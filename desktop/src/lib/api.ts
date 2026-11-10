import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Attachment,
  BackgroundFinished,
  BackgroundStarted,
  BackgroundTaskView,
  FileChange,
  Interaction,
  ModelCatalog,
  SessionMeta,
  SessionStatus,
  SessionUpdated,
  SessionView,
  SetupState,
  TextDelta,
  ThinkingDelta,
  ToolFinishedEvent,
  ToolStartedEvent,
  TurnChanges,
  TurnFailed,
  TurnFinished,
  TurnPage,
  TurnStarted,
  TurnVerifying,
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
  searchUrl: string | null = null,
): Promise<void> {
  return invoke("save_model", {
    provider,
    providerId,
    providerName,
    baseUrl,
    model,
    context,
    searchUrl,
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

/// Reopens a conversation, blank or not, under the current settings.
export function reopenSession(
  workspace: string | null,
  sessionId: string,
): Promise<SessionView> {
  return invoke("reopen_session", { workspace, sessionId });
}

export function branchSession(seq: number): Promise<SessionView> {
  return invoke("branch_session", { seq });
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

/// Copies the picked files into this conversation's attachment folder and
/// returns where they landed.
export function attachFiles(
  workspace: string,
  sessionId: string,
  paths: string[],
): Promise<Attachment[]> {
  return invoke("attach_files", { workspace, sessionId, paths });
}

export function cancelTurn(): Promise<boolean> {
  return invoke("cancel_turn");
}

export function undoLastTurn(): Promise<void> {
  return invoke("undo_last_turn");
}

/// The files the latest turn changed; empty when there is nothing to undo.
export function turnChanges(seq?: number | null): Promise<FileChange[]> {
  return invoke("turn_changes", { seq: seq ?? null });
}

export function sessionStatus(): Promise<SessionStatus> {
  return invoke("session_status");
}

/// Saves the permission override; the caller reopens the session to apply it.
export function savePermission(level: string): Promise<void> {
  return invoke("save_permission", { level });
}

/// Running and finished-but-unreported background commands. Reading this
/// never consumes a result; the engine hands those to the model itself.
export function backgroundTasks(): Promise<BackgroundTaskView[]> {
  return invoke("background_tasks");
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
    await listen<ThinkingDelta>("keelson:thinking-delta", (event) =>
      dispatch({ type: "thinking-delta", ...event.payload }),
    ),
  );
  unlisten.push(
    await listen<ToolStartedEvent>("keelson:tool-started", (event) =>
      dispatch({ type: "tool-started", ...event.payload }),
    ),
  );
  unlisten.push(
    await listen<ToolFinishedEvent>("keelson:tool-finished", (event) =>
      dispatch({ type: "tool-finished", ...event.payload }),
    ),
  );
  unlisten.push(
    await listen<BackgroundStarted>("keelson:background-started", (event) =>
      dispatch({
        type: "background-started",
        id: event.payload.id,
        program: event.payload.program,
        startedAt: Date.now(),
      }),
    ),
  );
  unlisten.push(
    await listen<BackgroundFinished>("keelson:background-finished", (event) =>
      dispatch({
        type: "background-finished",
        id: event.payload.id,
        program: event.payload.program,
        exitCode: event.payload.exitCode,
        durationMs: event.payload.durationMs,
        outputPath: event.payload.outputPath,
        startedAt: Date.now() - event.payload.durationMs,
      }),
    ),
  );
  unlisten.push(
    await listen<TurnVerifying>("keelson:turn-verifying", (event) =>
      dispatch({ type: "turn-verifying", ...event.payload }),
    ),
  );
  unlisten.push(
    await listen<TurnChanges>("keelson:turn-changes", (event) =>
      dispatch({ type: "turn-changes", seq: event.payload.seq }),
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
