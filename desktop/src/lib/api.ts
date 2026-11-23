import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { get } from "svelte/store";
import type {
  Attachment,
  BackgroundFinished,
  BackgroundStarted,
  BackgroundTaskView,
  EventTag,
  FileChange,
  Interaction,
  ModelCatalog,
  ModelFinished,
  RecordPaths,
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
import { dispatch, session } from "./session-store";
import { perf } from "./perf";

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
  keySlot: string | null = null,
  searchUrl: string | null = null,
): Promise<void> {
  return invoke("save_model", {
    provider,
    providerId,
    providerName,
    baseUrl,
    model,
    context,
    keySlot,
    searchUrl,
  });
}

export function savePreset(name: string): Promise<void> {
  return invoke("save_preset", { name });
}

export function applyPreset(id: string): Promise<void> {
  return invoke("apply_preset", { id });
}

export function deletePreset(id: string): Promise<void> {
  return invoke("delete_preset", { id });
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

/// Forgets every open conversation in the backend, cancelling the turns still
/// streaming into them. The next open builds a fresh engine, so a workspace
/// closes only on work the user asked for, never on a stranded turn.
export function forceRecover(): Promise<boolean> {
  return invoke("force_recover");
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

/// Where the per-turn step journal goes: `project`, `state` or `custom`.
export function saveJournal(mode: string, path: string): Promise<void> {
  return invoke("save_journal", { mode, path });
}

/// Where the next turn writes its two records (ADR 0063, 0064), for the
/// settings panel: the step log's database and the directory of task
/// documents.
export function recordPaths(): Promise<RecordPaths> {
  return invoke("record_paths");
}

/// Running and finished-but-unreported background commands. Reading this
/// never consumes a result; the engine hands those to the model itself.
export function backgroundTasks(): Promise<BackgroundTaskView[]> {
  return invoke("background_tasks");
}

/// Kills a running background command. Its exit still reaches the model at
/// the next safe point.
export function stopBackground(id: string): Promise<boolean> {
  return invoke("stop_background", { id });
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

/// Closes the card without answering; the turn continues with a deferred note.
export function dismissInteraction(id: string): Promise<void> {
  return invoke("dismiss_interaction", { id });
}

/// The interaction the backend is still waiting on, for a window that reloaded
/// after the original event.
export function pendingInteraction(): Promise<Interaction | null> {
  return invoke("pending_interaction");
}

/// Subscribes the reducer to every backend event. Call before opening a
/// session so no early stream is missed.
export async function listenToEvents(): Promise<UnlistenFn[]> {
  const unlisten: UnlistenFn[] = [];
  /// Live events name the conversation they belong to. Several conversations can
  /// have a turn running at once, and neither may paint into the other, so an
  /// event tagged with a different id is dropped before it reaches the reducer.
  const owned = (payload: EventTag): boolean =>
    payload.sessionId === undefined || payload.sessionId === get(session).sessionId;
  const on = async <T extends EventTag>(
    name: string,
    handle: (payload: T) => void,
  ): Promise<void> => {
    unlisten.push(
      await listen<T>(name, (event) => {
        if (!owned(event.payload)) return;
        perf.count(name);
        handle(event.payload);
      }),
    );
  };
  await on<TurnStarted>("keelson:turn-started", (payload) =>
    dispatch({ type: "turn-started", ...payload }),
  );
  await on<TextDelta>("keelson:text-delta", (payload) =>
    dispatch({ type: "text-delta", ...payload }),
  );
  await on<ThinkingDelta>("keelson:thinking-delta", (payload) =>
    dispatch({ type: "thinking-delta", ...payload }),
  );
  await on<ModelFinished>("keelson:model-finished", (payload) =>
    dispatch({ type: "model-finished", ...payload }),
  );
  await on<ToolStartedEvent>("keelson:tool-started", (payload) =>
    dispatch({ type: "tool-started", ...payload }),
  );
  await on<ToolFinishedEvent>("keelson:tool-finished", (payload) =>
    dispatch({ type: "tool-finished", ...payload }),
  );
  await on<BackgroundStarted>("keelson:background-started", (payload) =>
    dispatch({
      type: "background-started",
      id: payload.id,
      program: payload.program,
    }),
  );
  await on<BackgroundFinished>("keelson:background-finished", (payload) =>
    dispatch({
      type: "background-finished",
      id: payload.id,
      program: payload.program,
      exitCode: payload.exitCode,
      durationMs: payload.durationMs,
      outputPath: payload.outputPath,
      stopped: payload.stopped,
    }),
  );
  await on<TurnVerifying>("keelson:turn-verifying", (payload) =>
    dispatch({ type: "turn-verifying", ...payload }),
  );
  await on<TurnChanges>("keelson:turn-changes", (payload) =>
    dispatch({ type: "turn-changes", seq: payload.seq }),
  );
  await on<TurnFinished>("keelson:turn-finished", (payload) =>
    dispatch({ type: "turn-finished", turn: payload.turn }),
  );
  await on<TurnFailed>("keelson:turn-failed", (payload) =>
    dispatch({ type: "turn-failed", ...payload }),
  );
  await on<Interaction & EventTag>("keelson:interaction-requested", (payload) =>
    dispatch({ type: "interaction-requested", interaction: payload }),
  );
  await on<{ id: string } & EventTag>("keelson:interaction-resolved", (payload) =>
    dispatch({ type: "interaction-resolved", id: payload.id }),
  );
  await on<SessionUpdated & EventTag>("keelson:session-updated", (payload) =>
    dispatch({ type: "session-updated", meta: payload.meta }),
  );
  return unlisten;
}
