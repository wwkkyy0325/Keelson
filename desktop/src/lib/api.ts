import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Interaction,
  ModelCatalog,
  SessionMeta,
  SessionUpdated,
  SessionView,
  SetupState,
  TextDelta,
  TurnFailed,
  TurnFinished,
  TurnPage,
  TurnStarted,
} from "./protocol";
import { dispatch } from "./session-store";

export function setupState(): Promise<SetupState> {
  return invoke("setup_state");
}

export function modelCatalog(refresh = false): Promise<ModelCatalog> {
  return invoke("model_catalog", { refresh });
}

export function saveKey(key: string, providerId: string): Promise<void> {
  return invoke("save_key", { key, providerId });
}

export function saveModel(
  provider: string,
  providerId: string,
  providerName: string,
  baseUrl: string,
  model: string,
  context: number | null,
): Promise<void> {
  return invoke("save_model", {
    provider,
    providerId,
    providerName,
    baseUrl,
    model,
    context,
  });
}

export function saveLocale(locale: string): Promise<void> {
  return invoke("save_locale", { locale });
}

export function completeOnboarding(): Promise<void> {
  return invoke("complete_onboarding");
}

export function openSession(
  workspace: string | null,
  sessionId: string | null = null,
): Promise<SessionView> {
  return invoke("open_session", { workspace, sessionId });
}

export function saveRig(name: string): Promise<void> {
  return invoke("save_rig", { name });
}

export function newSession(workspace: string | null): Promise<SessionView> {
  return invoke("new_session", { workspace });
}

export function listSessions(workspace: string): Promise<SessionMeta[]> {
  return invoke("list_sessions", { workspace });
}

export function deleteSession(
  workspace: string,
  sessionId: string,
): Promise<void> {
  return invoke("delete_session", { workspace, sessionId });
}

export function renameSession(
  workspace: string,
  sessionId: string,
  title: string,
): Promise<SessionMeta> {
  return invoke("rename_session", { workspace, sessionId, title });
}

export function archiveSession(
  workspace: string,
  sessionId: string,
  archived: boolean,
): Promise<SessionMeta> {
  return invoke("archive_session", { workspace, sessionId, archived });
}

export function sendMessage(input: string): Promise<void> {
  return invoke("send_message", { input });
}

export function cancelTurn(): Promise<boolean> {
  return invoke("cancel_turn");
}

export function undoLastTurn(): Promise<void> {
  return invoke("undo_last_turn");
}

export function loadTurns(
  before: number | null,
  limit = 30,
): Promise<TurnPage> {
  return invoke("load_turns", { before, limit });
}

export function respondInteraction(
  id: string,
  option: string,
): Promise<void> {
  return invoke("respond_interaction", { id, option });
}

/// Subscribes the reducer to every backend event. Call before opening a
/// session so no early stream is missed.
export async function listenToEvents(): Promise<UnlistenFn[]> {
  const unlisten: UnlistenFn[] = [];
  unlisten.push(
    await listen<TurnStarted>("keelson:turn-started", (event) =>
      dispatch({ type: "turn-started", ...event.payload }),
    ),
  );
  unlisten.push(
    await listen<TextDelta>("keelson:text-delta", (event) =>
      dispatch({ type: "text-delta", ...event.payload }),
    ),
  );
  unlisten.push(
    await listen<TurnFinished>("keelson:turn-finished", (event) =>
      dispatch({ type: "turn-finished", turn: event.payload.turn }),
    ),
  );
  unlisten.push(
    await listen<TurnFailed>("keelson:turn-failed", (event) =>
      dispatch({ type: "turn-failed", ...event.payload }),
    ),
  );
  unlisten.push(
    await listen<Interaction>("keelson:interaction-requested", (event) =>
      dispatch({ type: "interaction-requested", interaction: event.payload }),
    ),
  );
  unlisten.push(
    await listen<{ id: string }>("keelson:interaction-resolved", (event) =>
      dispatch({ type: "interaction-resolved", id: event.payload.id }),
    ),
  );
  unlisten.push(
    await listen<SessionUpdated>("keelson:session-updated", (event) =>
      dispatch({ type: "session-updated", meta: event.payload.meta }),
    ),
  );
  return unlisten;
}
