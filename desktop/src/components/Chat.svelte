<script lang="ts">
  import {
    archiveSession,
    backgroundTasks,
    branchSession,
    cancelTurn,
    deleteSession,
    listSessions,
    newSession,
    openSession,
    renameSession,
    respondInteraction,
    savePermission,
    sendMessage,
    sessionStatus,
    turnChanges,
    undoLastTurn,
  } from "../lib/api";
  import { dispatch, session } from "../lib/session-store";
  import { rigTitle, t } from "../lib/i18n";
  import { baseName } from "../lib/paths";
  import AppHeader from "./AppHeader.svelte";
  import Composer from "./Composer.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import PermissionCard from "./PermissionCard.svelte";
  import Sidebar from "./Sidebar.svelte";
  import StatusBar from "./StatusBar.svelte";
  import Transcript from "./Transcript.svelte";
  import type { SessionStatus, Turn } from "../lib/protocol";

  let { onSettings }: { onSettings: () => void } = $props();

  let uiError = $state<string | null>(null);
  let answering = $state<string | null>(null);
  let confirmUndo = $state(false);
  /// Text a branch opens with, handed to the composer through its keyed mount.
  let prefillDraft = $state("");
  let undoDone = $state(false);
  /// Files the latest turn changed, from its checkpoint pair.
  // The diff belongs to the turn that made it; the latest turn's copy drives
  // the undo button and the card above the actions.
  let latestChanges = $derived($session.turns.at(-1)?.changes ?? []);
  /// ArrowUp/ArrowDown in the composer walk the inputs already sent.
  let promptHistory = $derived($session.turns.map((turn) => turn.input));
  /// Background commands outlive a turn, so the view polls the engine while
  /// any is known. A result stays until the engine hands it to the model.
  let backgroundTimer: ReturnType<typeof setInterval> | null = null;

  function pollBackground(): void {
    if (backgroundTimer !== null) return;
    const sync = async (): Promise<void> => {
      try {
        const tasks = await backgroundTasks();
        const now = Date.now();
        dispatch({
          type: "background-synced",
          tasks: tasks.map((task) => ({
            id: task.id,
            program: task.program,
            startedAt: now - task.durationMs,
            ok: task.running ? null : task.exitCode === 0,
            durationMs: task.durationMs,
            outputPath: task.outputPath,
          })),
        });
        if (tasks.length === 0 && backgroundTimer !== null) {
          clearInterval(backgroundTimer);
          backgroundTimer = null;
        }
      } catch {
        // A closed session stops the poll; the next live event starts it
        // again for the session that owns the task.
        if (backgroundTimer !== null) {
          clearInterval(backgroundTimer);
          backgroundTimer = null;
        }
      }
    };
    void sync();
    backgroundTimer = setInterval(sync, 1000);
  }

  $effect(() => {
    if ($session.background.length > 0) {
      pollBackground();
    }
  });
  let undoable = $derived(
    !$session.running && !undoDone && latestChanges.length > 0,
  );
  /// Permission, connection and context usage for the composer's status line.
  let status = $state<SessionStatus | null>(null);

  $effect(() => {
    $session.sessionId;
    $session.turns.length;
    undoDone = false;
    confirmUndo = false;
  });

  // The change summary is the checkpoint diff, not a guess from the tool
  // names: a turn that wrote nothing has nothing to undo. The epoch covers a
  // reconnect that keeps the same conversation id.
  $effect(() => {
    const epoch = $session.epoch;
    const count = $session.turns.length;
    const pendingChanges = $session.changesEpoch;
    const undone = undoDone;
    void epoch;
    void count;
    void pendingChanges;
    void undone;
    void loadChanges();
  });

  // Same trigger for the status line: it reads the engine's permission and
  // context, so every open and every finished turn refreshes it.
  $effect(() => {
    const epoch = $session.epoch;
    const count = $session.turns.length;
    void epoch;
    void count;
    void loadStatus();
  });

  async function loadChanges(): Promise<void> {
    try {
      const seq = $session.changedSeq ?? $session.turns.at(-1)?.seq ?? null;
      const loaded = await turnChanges(seq);
      if (seq !== null) {
        dispatch({ type: "turn-changes-loaded", seq, changes: loaded });
      }
    } catch {
      // A closed session or a missing checkpoint leaves the turn as it was.
    }
  }

  async function loadStatus(): Promise<void> {
    try {
      status = await sessionStatus();
    } catch {
      status = null;
    }
  }

  async function changePermission(level: string): Promise<void> {
    uiError = null;
    try {
      await savePermission(level);
      await loadStatus();
    } catch (error) {
      uiError = errText(error);
    }
  }

  let currentTitle = $derived(
    $session.sessions.find((item) => item.id === $session.sessionId)?.title ||
      $t("sidebar.untitled"),
  );
  let projectName = $derived(baseName($session.workspace) || $session.workspace);
  let currentArchived = $derived(
    $session.sessions.find((item) => item.id === $session.sessionId)
      ?.archived ?? false,
  );

  async function undo(): Promise<void> {
    confirmUndo = false;
    uiError = null;
    try {
      await undoLastTurn();
      undoDone = true;
    } catch (error) {
      uiError = errText(error);
    }
  }

  async function openProject(path: string): Promise<void> {
    if ($session.running) return;
    uiError = null;
    try {
      const view = await openSession(path, null);
      dispatch({ type: "session-opened", view });
    } catch (error) {
      uiError = errText(error);
    }
  }

  async function newConversation(): Promise<void> {
    if ($session.running) return;
    uiError = null;
    prefillDraft = "";
    try {
      const view = await newSession($session.workspace);
      dispatch({ type: "session-opened", view });
    } catch (error) {
      uiError = errText(error);
    }
  }

  async function selectConversation(id: string): Promise<void> {
    if (id === $session.sessionId || $session.running) return;
    uiError = null;
    prefillDraft = "";
    try {
      const view = await openSession($session.workspace, id);
      dispatch({ type: "session-opened", view });
    } catch (error) {
      uiError = errText(error);
    }
  }

  async function branchFrom(turn: Turn): Promise<void> {
    if ($session.running) return;
    uiError = null;
    const name = `${currentTitle}${$t("turn.branchTitle")}`;
    try {
      const view = await branchSession(turn.seq);
      prefillDraft = turn.input;
      dispatch({ type: "session-opened", view });
      await renameSession(view.workspace, view.sessionId, name);
      const sessions = await listSessions(view.workspace);
      dispatch({ type: "sessions-loaded", sessions });
    } catch (error) {
      uiError = errText(error);
    }
  }

  async function removeConversation(id: string): Promise<void> {
    if ($session.running) return;
    uiError = null;
    const wasCurrent = id === $session.sessionId;
    try {
      if (wasCurrent) {
        const view = await newSession($session.workspace);
        dispatch({ type: "session-opened", view });
      }
      await deleteSession($session.workspace, id);
      const sessions = await listSessions($session.workspace);
      dispatch({ type: "sessions-loaded", sessions });
    } catch (error) {
      uiError = errText(error);
    }
  }

  async function renameConversation(id: string, title: string): Promise<void> {
    uiError = null;
    try {
      await renameSession($session.workspace, id, title);
      const sessions = await listSessions($session.workspace);
      dispatch({ type: "sessions-loaded", sessions });
    } catch (error) {
      uiError = errText(error);
    }
  }

  async function archiveConversation(
    id: string,
    archived: boolean,
  ): Promise<void> {
    if ($session.running) return;
    uiError = null;
    const wasCurrent = id === $session.sessionId;
    try {
      await archiveSession($session.workspace, id, archived);
      const sessions = await listSessions($session.workspace);
      dispatch({ type: "sessions-loaded", sessions });
      if (wasCurrent && archived) {
        const view = await openSession($session.workspace, null);
        dispatch({ type: "session-opened", view });
      }
    } catch (error) {
      uiError = errText(error);
    }
  }

  async function send(text: string): Promise<void> {
    uiError = null;
    // Writing to an archived conversation brings it back: the alternative was
    // a read-only view that explained itself above the composer.
    if (currentArchived && $session.sessionId) {
      await archiveSession($session.workspace, $session.sessionId, false);
      const sessions = await listSessions($session.workspace);
      dispatch({ type: "sessions-loaded", sessions });
    }
    await sendMessage(text);
  }

  async function cancel(): Promise<void> {
    await cancelTurn();
  }

  async function answer(option: string): Promise<void> {
    const pending = $session.pendingInteraction;
    if (!pending || answering) return;
    answering = option;
    uiError = null;
    try {
      await respondInteraction(pending.id, option);
    } catch (error) {
      uiError = errText(error);
    } finally {
      answering = null;
    }
  }

  function errText(error: unknown): string {
    if (typeof error === "string") return error;
    if (error instanceof Error) return error.message;
    return String(error);
  }
</script>

<div class="app">
  <Sidebar
    workspace={$session.workspace}
    projects={$session.projects}
    sessions={$session.sessions}
    sessionId={$session.sessionId}
    running={$session.running !== null}
    onNewChat={newConversation}
    onSelect={selectConversation}
    onRename={renameConversation}
    onArchive={archiveConversation}
    onDelete={removeConversation}
    onProject={openProject}
  />

  <div class="main">
    <AppHeader
      title={currentTitle}
      archived={currentArchived}
      {onSettings}
    />

    <Transcript
      onError={(message) => (uiError = message)}
      error={uiError ?? $session.lastError}
      {projectName}
      changes={latestChanges}
      {undoable}
      onUndo={() => (confirmUndo = true)}
      onBranch={branchFrom}
    />

    <footer>
      {#if $session.pendingInteraction}
        <PermissionCard
          interaction={$session.pendingInteraction}
          {answering}
          onAnswer={answer}
        />
      {/if}

      {#key $session.sessionId}
        <Composer
          running={$session.running !== null}
          onSend={send}
          onCancel={cancel}
          onError={(message) => (uiError = message)}
          initialDraft={prefillDraft}
          history={promptHistory}
        />
      {/key}

      <StatusBar
        {status}
        rigTitle={status ? $rigTitle(status.rig, status.rig) : ""}
        busy={$session.running !== null}
        onSettings={onSettings}
        onPermission={changePermission}
      />
    </footer>
  </div>

  {#if confirmUndo}
    <ConfirmDialog
      title={$t("app.undoTitle")}
      message={$t("app.undoMessage")}
      confirmLabel={$t("app.undoConfirm")}
      onConfirm={undo}
      onCancel={() => (confirmUndo = false)}
    />
  {/if}
</div>
