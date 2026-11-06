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

pub struct DesktopState {
    pub engine: Mutex<Option<Arc<Engine>>>,
    pub cancel: Mutex<Option<CancelToken>>,
    /// The turn currently streaming; the delta forwarder reads it.
    pub current_seq: Arc<AtomicU32>,
    pub interactions: Arc<DesktopInteractions>,
    pub config: Mutex<DesktopConfig>,
}

impl DesktopState {
    pub fn new(app: AppHandle) -> Self {
        Self {
            engine: Mutex::new(None),
            cancel: Mutex::new(None),
            current_seq: Arc::new(AtomicU32::new(0)),
            interactions: Arc::new(DesktopInteractions::new(app)),
            config: Mutex::new(config::load_config()),
        }
    }
}

struct Pending {
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
}

/// Kernel interactions become Tauri events. The turn waits on a oneshot until
/// the window answers by id.
pub struct DesktopInteractions {
    app: AppHandle,
    pending: Mutex<HashMap<String, Pending>>,
    next_id: AtomicU64,
}

impl DesktopInteractions {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    pub fn respond(&self, id: &str, option: &str) -> Result<(), String> {
        let pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(id)
            .ok_or_else(|| format!("no pending interaction `{id}`"))?;
        let response = response_for(&pending.request, option);
        let _ = pending.reply.send(response);
        let _ = self
            .app
            .emit(EV_INTERACTION_RESOLVED, serde_json::json!({ "id": id }));
        Ok(())
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
        let payload = InteractionRequested {
            id: id.clone(),
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
        };
        let (reply, answer) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id.clone(), Pending { request, reply });
        let _ = self.app.emit(EV_INTERACTION_REQUESTED, payload);
        tokio::select! {
            _ = cancel.cancelled() => {
                self.pending
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .remove(&id);
                InteractionResponse::Cancelled
            }
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
