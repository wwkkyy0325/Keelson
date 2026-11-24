//! Shared desktop state: the open engine and the kernel interaction port.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use keel_core::{
    CancelToken, GrantScope, InteractionKind, InteractionPort, InteractionRequest,
    InteractionResponse,
};
use keelson::engine::Engine;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

use crate::config::{self, DesktopConfig};

pub const EV_INTERACTION_REQUESTED: &str = "keelson:interaction-requested";
pub const EV_INTERACTION_RESOLVED: &str = "keelson:interaction-resolved";

/// Grants made from the desktop expire even if the window stays open.
const GRANT_TTL: Duration = Duration::from_secs(8 * 60 * 60);

/// The conversation the engine is writing to. The session row is refreshed
/// from it after every turn.
#[derive(Clone)]
pub struct CurrentSession {
    pub id: String,
    pub workspace: PathBuf,
    pub state_dir: PathBuf,
    /// The archive flag at open, updated by the archive command. The send path
    /// reads this instead of parsing the conversation document.
    pub archived: bool,
}

/// One conversation the app has open: its engine, its cancel token and the
/// sequence of the turn streaming into it.
///
/// These travel together on purpose. A conversation owns its work: the
/// background commands it started belong to its engine, so looking at another
/// conversation must not take them down with it.
pub struct RunningTurn {
    pub seq: u32,
    pub input: String,
    /// The answer as far as it has streamed.
    ///
    /// It is what a window that comes back to this conversation still holds,
    /// and what the note on disk keeps for the next process (ADR 0083).
    pub partial: String,
}

/// How much of a streaming answer the host keeps.
///
/// The beginning is what the reader has already read, so that is the part worth
/// keeping when an answer runs past the bound. Eight thousand bytes matches the
/// document's cap on a tool output; what the number buys here is that rewriting
/// the note while a turn streams for minutes stays cheap.
pub const PARTIAL_BYTES: usize = 8_000;

impl RunningTurn {
    /// Keeps the head of the answer, up to the bound.
    pub fn push_partial(&mut self, text: &str) {
        let room = PARTIAL_BYTES.saturating_sub(self.partial.len());
        if room == 0 {
            return;
        }
        if text.len() <= room {
            self.partial.push_str(text);
            return;
        }
        // A bound in bytes must not cut a character in half: the note is JSON
        // and a lone continuation byte would make it unreadable.
        let mut cut = room;
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        self.partial.push_str(&text[..cut]);
    }
}

/// The connection a conversation's engine was built with.
///
/// An engine cannot change provider or model under itself, so a conversation
/// opened under the old model keeps running it until its engine is rebuilt. The
/// status line reads this snapshot, not the setting: reporting the model the
/// settings now hold while the engine still calls the old one is the same lie
/// that made the rig look stuck for months (ADR 0039).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Connection {
    pub provider: String,
    pub provider_name: String,
    pub model: String,
    pub base_url: String,
    pub context_window: Option<u32>,
    /// The web tools' endpoint and key, which decide whether they exist at all.
    pub search_url: String,
    pub search_key: u64,
    /// The key the engine was built with, reduced to a fingerprint: a changed
    /// key has to rebuild the engine, but a snapshot is not a place to keep a
    /// secret.
    pub key: u64,
}

pub struct OpenConversation {
    pub id: String,
    /// Where the conversation's document and turns live. A command that names
    /// this conversation can do its bookkeeping from here, without reading the
    /// window's pointer, which may already have moved on.
    pub workspace: PathBuf,
    pub state_dir: PathBuf,
    pub engine: Arc<Engine>,
    pub cancel: Mutex<Option<CancelToken>>,
    pub seq: Arc<AtomicU32>,
    /// The turn streaming into this conversation, if one is.
    pub running: Mutex<Option<RunningTurn>>,
    /// What the engine in here calls: the setting at the moment it was built.
    pub connection: Connection,
}

/// How many conversations may keep an engine, including the visible one. A
/// conversation with a running turn or a live background command is never
/// evicted; it is counted again once that work ends.
const OPEN_LIMIT: usize = 8;

pub struct DesktopState {
    /// Every conversation this window has opened, by id.
    pub open: Mutex<HashMap<String, Arc<OpenConversation>>>,
    pub interactions: Arc<DesktopInteractions>,
    pub config: Mutex<DesktopConfig>,
    pub current: Mutex<Option<CurrentSession>>,
    /// Serializes index reads and writes; the session documents themselves
    /// are replaced atomically.
    pub session_io: Mutex<()>,
}

impl DesktopState {
    pub fn new(app: AppHandle) -> Self {
        Self {
            open: Mutex::new(HashMap::new()),
            interactions: Arc::new(DesktopInteractions::new(app)),
            config: Mutex::new(config::load_config()),
            current: Mutex::new(None),
            session_io: Mutex::new(()),
        }
    }

    /// The conversation the window is showing.
    pub fn current_id(&self) -> Option<String> {
        self.current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(|current| current.id.clone())
    }

    pub fn open_of(&self, id: &str) -> Option<Arc<OpenConversation>> {
        self.open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(id)
            .cloned()
    }

    /// Applies a setting to every conversation the window holds open. A
    /// setting that belongs to the app is heard by the engines already built
    /// from it, not only by the next one.
    pub fn each_open(&self, mut apply: impl FnMut(&Arc<OpenConversation>)) {
        let open = self
            .open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for conversation in open.values() {
            apply(conversation);
        }
    }

    /// The open conversation the window shows, or the reason there is none.
    pub fn current_open(&self) -> Result<Arc<OpenConversation>, String> {
        let id = self
            .current_id()
            .ok_or_else(|| "还没有打开会话".to_owned())?;
        self.open_of(&id)
            .ok_or_else(|| "会话已关闭，请重新打开".to_owned())
    }

    /// Keeps a conversation open. A conversation that is still working is never
    /// evicted; when the map is full the eviction looks for an idle one.
    pub fn install_open(&self, conversation: Arc<OpenConversation>) {
        let mut open = self
            .open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let id = conversation.id.clone();
        open.insert(id.clone(), conversation);
        if open.len() <= OPEN_LIMIT {
            return;
        }
        let idle = open
            .iter()
            .find(|(other, conversation)| {
                *other != &id
                    && conversation
                        .cancel
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .is_none()
                    && conversation.engine.background_tasks().is_empty()
            })
            .map(|(other, _)| other.clone());
        if let Some(idle) = idle {
            open.remove(&idle);
        }
    }
    /// Forgets every open conversation and the current one, cancelling a turn
    /// that is still streaming. A forced recovery drops the engines instead of
    /// waiting for a turn that will not unwind; the next open rebuilds them,
    /// and the abandoned turn writes its result into the document as always.
    pub fn forget_open(&self) -> bool {
        let had_current = self
            .current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            .is_some();
        let mut open = self
            .open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for (_, conversation) in open.drain() {
            if let Some(token) = conversation
                .cancel
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take()
            {
                token.cancel();
            }
        }
        had_current
    }
}

struct Pending {
    id: String,
    request: InteractionRequest,
    reply: oneshot::Sender<InteractionResponse>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InteractionOptionView {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InteractionRequested {
    pub id: String,
    pub kind: String,
    pub prompt: String,
    pub options: Vec<InteractionOptionView>,
    /// Id of the option the model recommends, when it named one.
    pub recommended: Option<String>,
}

/// Kernel interactions become Tauri events. The turn waits on a oneshot until
/// the window answers by id.
pub struct DesktopInteractions {
    app: AppHandle,
    pending: Arc<Mutex<HashMap<String, Pending>>>,
    next_id: AtomicU64,
}

impl DesktopInteractions {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(1),
        }
    }

    /// Answers by option. A late answer for an interaction that was cancelled
    /// or timed out is a no-op: the turn already moved on.
    pub fn respond(&self, id: &str, option: &str) -> Result<(), String> {
        let Some(pending) = self.take(id) else {
            return Ok(());
        };
        let response = response_for(&pending.request, option);
        let _ = pending.reply.send(response);
        Ok(())
    }

    /// Closes the card without an answer. The turn continues; the driver tells
    /// the model the item was deferred and to list it in the final report.
    pub fn dismiss(&self, id: &str) -> Result<(), String> {
        let Some(pending) = self.take(id) else {
            return Ok(());
        };
        let _ = pending.reply.send(InteractionResponse::Deferred);
        Ok(())
    }

    /// The interaction the turn is waiting on, if any. A reloaded window asks
    /// once on session open because it missed the original event.
    pub fn current(&self) -> Option<InteractionRequested> {
        let pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        pending
            .values()
            .next()
            .map(|pending| requested_view(&pending.id, &pending.request))
    }

    fn take(&self, id: &str) -> Option<Pending> {
        let pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(id);
        if pending.is_some() {
            let _ = self
                .app
                .emit(EV_INTERACTION_RESOLVED, serde_json::json!({ "id": id }));
        }
        pending
    }
}

/// Removes a pending interaction when its turn stops waiting: cancellation,
/// the broker's deadline, or a dropped engine all drop the request future.
/// Without this the card outlives the turn and the user can never close it.
struct InteractionGuard {
    pending: Arc<Mutex<HashMap<String, Pending>>>,
    app: AppHandle,
    id: String,
}

impl Drop for InteractionGuard {
    fn drop(&mut self) {
        let removed = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.id)
            .is_some();
        if removed {
            let _ = self.app.emit(
                EV_INTERACTION_RESOLVED,
                serde_json::json!({ "id": self.id }),
            );
        }
    }
}

fn requested_view(id: &str, request: &InteractionRequest) -> InteractionRequested {
    InteractionRequested {
        id: id.to_owned(),
        kind: request.kind.as_str().to_owned(),
        prompt: request.prompt.clone(),
        options: request
            .options
            .iter()
            .map(|option| InteractionOptionView {
                id: option.id.clone(),
                label: option.label.clone(),
            })
            .collect(),
        recommended: request.recommended.clone(),
    }
}

#[async_trait]
impl InteractionPort for DesktopInteractions {
    async fn request(
        &self,
        request: InteractionRequest,
        cancel: &CancelToken,
    ) -> InteractionResponse {
        if request.kind != InteractionKind::PermissionApproval && request.options.is_empty() {
            return InteractionResponse::Deferred;
        }
        let id = format!("i-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let payload = requested_view(&id, &request);
        let (reply, answer) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                id.clone(),
                Pending {
                    id: id.clone(),
                    request,
                    reply,
                },
            );
        let _guard = InteractionGuard {
            pending: Arc::clone(&self.pending),
            app: self.app.clone(),
            id: id.clone(),
        };
        let _ = self.app.emit(EV_INTERACTION_REQUESTED, payload);
        tokio::select! {
            _ = cancel.cancelled() => InteractionResponse::Cancelled,
            result = answer => result.unwrap_or(InteractionResponse::Deferred),
        }
    }
}

fn response_for(request: &InteractionRequest, option: &str) -> InteractionResponse {
    if request.kind == InteractionKind::PermissionApproval {
        return match option {
            "deny" => InteractionResponse::Rejected {
                reason: "user denied".to_owned(),
            },
            "allow_once" => InteractionResponse::Approved {
                scope: GrantScope::Once,
            },
            "allow_scope" => InteractionResponse::Approved {
                scope: GrantScope::PathPrefix {
                    prefix: request.path.clone().unwrap_or_else(|| PathBuf::from(".")),
                    ttl: GRANT_TTL,
                },
            },
            "full_auto" => InteractionResponse::Approved {
                scope: GrantScope::FullAutoSession { ttl: GRANT_TTL },
            },
            other => InteractionResponse::Selected {
                option: other.to_owned(),
            },
        };
    }
    InteractionResponse::Selected {
        option: option.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn() -> RunningTurn {
        RunningTurn {
            seq: 1,
            input: "go".to_owned(),
            partial: String::new(),
        }
    }

    /// The buffer keeps the head of the answer and stops at the bound, on a
    /// character boundary: the note it feeds is JSON, so half a character would
    /// make it unreadable (ADR 0083).
    #[test]
    fn the_partial_answer_starts_at_the_beginning_and_stops_at_the_bound() {
        let mut turn = turn();
        turn.push_partial("hello ");
        assert_eq!(turn.partial, "hello ");

        // Far more than the bound in one push.
        turn.push_partial(&"é".repeat(PARTIAL_BYTES));
        assert_eq!(turn.partial.len(), PARTIAL_BYTES);
        assert!(turn.partial.starts_with("hello "));
        assert!(turn.partial.is_char_boundary(turn.partial.len()));

        // A full buffer takes nothing more.
        let full = turn.partial.clone();
        turn.push_partial("more");
        assert_eq!(turn.partial, full);
    }

    /// A push that would cross the bound keeps its fitting prefix and stops on a
    /// character boundary — never half a character, and never past the bound.
    #[test]
    fn a_push_that_crosses_the_bound_stops_on_a_character_boundary() {
        let mut one = turn();
        one.partial = "x".repeat(PARTIAL_BYTES - 1);
        one.push_partial("日本語");
        // One byte of room fits no whole character, so nothing is kept.
        assert_eq!(one.partial.len(), PARTIAL_BYTES - 1);

        let mut four = turn();
        four.partial = "x".repeat(PARTIAL_BYTES - 4);
        four.push_partial("日本語");
        // Four bytes of room hold the first character (three bytes) and no more.
        assert_eq!(four.partial.len(), PARTIAL_BYTES - 1);
        assert!(four.partial.ends_with('日'));
        assert!(four.partial.is_char_boundary(four.partial.len()));
    }
}
