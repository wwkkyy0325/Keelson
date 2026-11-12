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

pub struct DesktopState {
    pub engine: Mutex<Option<Arc<Engine>>>,
    pub cancel: Mutex<Option<CancelToken>>,
    /// The turn currently streaming; the delta forwarder reads it.
    pub current_seq: Arc<AtomicU32>,
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
            engine: Mutex::new(None),
            cancel: Mutex::new(None),
            current_seq: Arc::new(AtomicU32::new(0)),
            interactions: Arc::new(DesktopInteractions::new(app)),
            config: Mutex::new(config::load_config()),
            current: Mutex::new(None),
            session_io: Mutex::new(()),
        }
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
