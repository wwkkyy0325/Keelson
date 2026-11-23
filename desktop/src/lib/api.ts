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
import type { Action } from "./session";
import { dispatch, dispatchTo, focus } from "./session-store";
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

export function branchSession(seq: number, sessionId: string | null = shownSession()): Promise<SessionView> {
  return invoke("branch_session", { seq, sessionId });
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
  // The turn belongs to the conversation on screen, so the window names it
  // rather than letting the host follow a pointer that a switch can leave
  // behind: a turn that ran elsewhere streamed into a view that dropped it.
  return invoke("send_message", { input, sessionId: shownSession() });
}

/// The conversation the window is showing, or null before one is open. The
/// focus holds the id the window was opened with, so this answer cannot lag the
/// state it is read from.
function shownSession(): string | null {
  return get(focus) || null;
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

/// Cancels the turn of the conversation on screen. The boot screen passes
/// `null`: it cancels whatever the host still has open, which is the turn that
/// blocked the open in the first place.
export function cancelTurn(
  sessionId: string | null = shownSession(),
): Promise<boolean> {
  return invoke("cancel_turn", { sessionId });
}

export function undoLastTurn(): Promise<void> {
  return invoke("undo_last_turn", { sessionId: shownSession() });
}

/// The files the latest turn changed; empty when there is nothing to undo.
export function turnChanges(seq?: number | null): Promise<FileChange[]> {
  return invoke("turn_changes", { seq: seq ?? null, sessionId: shownSession() });
}

export function sessionStatus(): Promise<SessionStatus> {
  return invoke("session_status", { sessionId: shownSession() });
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
  return invoke("background_tasks", { sessionId: shownSession() });
}

/// Kills a running background command. Its exit still reaches the model at
/// the next safe point.
export function stopBackground(id: string): Promise<boolean> {
  return invoke("stop_background", { id, sessionId: shownSession() });
}

export function loadTurns(
  before: number | null,
  limit = 30,
): Promise<TurnPage> {
  return invoke("load_turns", { before, limit, sessionId: shownSession() });
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
  /// Live events name the conversation they belong to, and that name decides
  /// where the action lands — not the conversation on screen. A turn keeps
  /// running in the conversation the user switched away from; its transcript has
  /// to be there, whole, when they switch back.
  const on = async <T extends EventTag>(
    name: string,
    action: (payload: T) => Action,
  ): Promise<void> => {
    unlisten.push(
      await listen<T>(name, (event) => {
        perf.count(name);
        const id = event.payload.sessionId;
        if (id === undefined || id === "") dispatch(action(event.payload));
        else dispatchTo(id, action(event.payload));
      }),
    );
  };
  await on<TurnStarted>("keelson:turn-started", (payload) => ({
    type: "turn-started",
    ...payload,
  }));
  await on<TextDelta>("keelson:text-delta", (payload) => ({
    type: "text-delta",
    ...payload,
  }));
  await on<ThinkingDelta>("keelson:thinking-delta", (payload) => ({
    type: "thinking-delta",
    ...payload,
  }));
  await on<ModelFinished>("keelson:model-finished", (payload) => ({
    type: "model-finished",
    ...payload,
  }));
  await on<ToolStartedEvent>("keelson:tool-started", (payload) => ({
    type: "tool-started",
    ...payload,
  }));
  await on<ToolFinishedEvent>("keelson:tool-finished", (payload) => ({
    type: "tool-finished",
    ...payload,
  }));
  await on<BackgroundStarted>("keelson:background-started", (payload) => ({
    type: "background-started",
    id: payload.id,
    program: payload.program,
  }));
  await on<BackgroundFinished>("keelson:background-finished", (payload) => ({
    type: "background-finished",
    id: payload.id,
    program: payload.program,
    exitCode: payload.exitCode,
    durationMs: payload.durationMs,
    outputPath: payload.outputPath,
    stopped: payload.stopped,
  }));
  await on<TurnVerifying>("keelson:turn-verifying", (payload) => ({
    type: "turn-verifying",
    ...payload,
  }));
  await on<TurnChanges>("keelson:turn-changes", (payload) => ({
    type: "turn-changes",
    seq: payload.seq,
  }));
  await on<TurnFinished>("keelson:turn-finished", (payload) => ({
    type: "turn-finished",
    turn: payload.turn,
  }));
  await on<TurnFailed>("keelson:turn-failed", (payload) => ({
    type: "turn-failed",
    ...payload,
  }));
  await on<Interaction & EventTag>(
    "keelson:interaction-requested",
    (payload) => ({ type: "interaction-requested", interaction: payload }),
  );
  await on<{ id: string } & EventTag>(
    "keelson:interaction-resolved",
    (payload) => ({ type: "interaction-resolved", id: payload.id }),
  );
  await on<SessionUpdated & EventTag>("keelson:session-updated", (payload) => ({
    type: "session-updated",
    meta: payload.meta,
  }));
  return unlisten;
}
