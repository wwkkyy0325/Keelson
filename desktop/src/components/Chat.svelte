<script lang="ts">
  import { untrack } from "svelte";
  import {
    applyPreset,
    archiveSession,
    attachFiles,
    backgroundTasks,
    branchSession,
    cancelTurn,
    deleteSession,
    dismissInteraction,
    listSessions,
    newSession,
    openSession,
    pendingInteraction,
    renameSession,
    reopenSession,
    respondInteraction,
    savePermission,
    savePreset,
    saveRig,
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
  import type {
    Preset,
    RigOption,
    SessionStatus,
    Turn,
  } from "../lib/protocol";

  let {
    onSettings,
    rigs = [],
    presets = [],
    activePreset = null,
    onPresetsChanged,
  }: {
    onSettings: () => void;
    rigs?: RigOption[];
    presets?: Preset[];
    activePreset?: string | null;
    onPresetsChanged: () => Promise<void>;
  } = $props();

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
  // The store replaces its value on every dispatch, so an effect reading
  // `$session.x` directly re-runs on every change anywhere in the state.
  // These derived values notify only when the value they name changes.
  let backgroundEpoch = $derived($session.epoch);
  let backgroundVisible = $derived($session.background.length > 0);

  async function syncBackground(): Promise<void> {
    const epoch = $session.epoch;
    try {
      const tasks = await backgroundTasks();
      // A switch mid-request must not paint the previous session's rows.
      if ($session.epoch !== epoch) return;
      dispatch({
        type: "background-synced",
        tasks: tasks.map((task) => ({
          id: task.id,
          program: task.program,
          ok: task.running ? null : task.exitCode === 0,
          durationMs: task.durationMs,
          outputPath: task.outputPath,
        })),
      });
    } catch {
      // A transient failure must not freeze the row: the next poll retries.
    }
  }

  function pollBackground(): void {
    if (backgroundTimer !== null) return;
    void syncBackground();
    backgroundTimer = setInterval(() => void syncBackground(), 1000);
  }

  // A background command outlives the view. On every session open, reconcile
  // first: a task started before a switch or a reconnect has to reappear, and
  // its start event will never be replayed.
  $effect(() => {
    void backgroundEpoch;
    untrack(() => {
      void syncBackground();
    });
  });

  $effect(() => {
    if (backgroundVisible) {
      pollBackground();
    } else if (backgroundTimer !== null) {
      // A new session, or a drained task, clears the rows; the poll goes
      // with them.
      clearInterval(backgroundTimer);
      backgroundTimer = null;
    }
    return () => {
      // The conversation closed: no more reads against a gone engine.
      if (backgroundTimer !== null) {
        clearInterval(backgroundTimer);
        backgroundTimer = null;
      }
    };
  });
  let undoable = $derived(
    !$session.running && !undoDone && latestChanges.length > 0,
  );
  /// Permission, connection and context usage for the composer's status line.
  let status = $state<SessionStatus | null>(null);

  $effect(() => {
    activeSessionId;
    turnCount;
    undoDone = false;
    confirmUndo = false;
  });

  // The store changes on every streamed delta; reading it from an effect would
  // run that effect (and its backend calls) for each token. These derived
  // values notify only when they really change.
  let activeSessionId = $derived($session.sessionId);
  let turnCount = $derived($session.turns.length);
  let sessionEpoch = $derived($session.epoch);
  let changesEpoch = $derived($session.changesEpoch);
  let changedSeq = $derived($session.changedSeq);

  // The change summary is the checkpoint diff, not a guess from the tool
  // names: a turn that wrote nothing has nothing to undo. The epoch covers a
  // reconnect that keeps the same conversation id.
  $effect(() => {
    void sessionEpoch;
    void turnCount;
    void changesEpoch;
    void changedSeq;
    void undoDone;
    untrack(() => {
      void loadChanges();
    });
  });

  // Same trigger for the status line: it reads the engine's permission and
  // context, so every open and every finished turn refreshes it.
  $effect(() => {
    void sessionEpoch;
    void turnCount;
    untrack(() => {
      void loadStatus();
    });
  });

  // A reload misses the interaction event while the turn is still waiting on
  // the answer. Ask the backend once per open and restore the card.
  $effect(() => {
    void sessionEpoch;
    const id = activeSessionId;
    untrack(() => {
      if (!id) return;
      void restoreInteraction();
    });
  });

  async function restoreInteraction(): Promise<void> {
    try {
      const pending = await pendingInteraction();
      if (pending) {
        dispatch({ type: "interaction-requested", interaction: pending });
      }
    } catch {
      // The engine may be closing; the next open asks again.
    }
  }

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

  /// A preset switch writes the connection, then reopens the same
  /// conversation: its history stays and the next turn uses the new model.
  /// The rig part lands with the next conversation, because a conversation
  /// with a turn keeps the rig it started with.
  async function choosePreset(id: string): Promise<void> {
    uiError = null;
    try {
      await applyPreset(id);
      const sessionId = $session.sessionId;
      if (sessionId) {
        const view = await reopenSession($session.workspace || null, sessionId);
        dispatch({ type: "session-opened", view });
      }
      await onPresetsChanged();
    } catch (error) {
      uiError = errText(error);
    }
  }

  /// Saves the connection as it is now; an unnamed preset takes the model.
  async function storePreset(name: string): Promise<void> {
    uiError = null;
    try {
      await savePreset(name || status?.model || "预设");
      await onPresetsChanged();
    } catch (error) {
      uiError = errText(error);
    }
  }

  /// A conversation with no turns is the only place the rig is chosen; after
  /// the first turn it belongs to the conversation and never changes.
  let emptyConversation = $derived(
    $session.turns.length === 0 && $session.running === null,
  );

  async function chooseRig(name: string): Promise<void> {
    if (!emptyConversation || !$session.sessionId || status?.rig === name) {
      return;
    }
    uiError = null;
    try {
      await saveRig(name);
      const view = await reopenSession(
        $session.workspace || null,
        $session.sessionId,
      );
      dispatch({ type: "session-opened", view });
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
      dispatch({ type: "interaction-resolved", id: pending.id });
    } catch (error) {
      uiError = errText(error);
    } finally {
      answering = null;
    }
  }

  async function dismiss(): Promise<void> {
    const pending = $session.pendingInteraction;
    if (!pending || answering) return;
    answering = "dismiss";
    uiError = null;
    try {
      await dismissInteraction(pending.id);
      dispatch({ type: "interaction-resolved", id: pending.id });
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
          onDismiss={dismiss}
        />
      {/if}

      {#key $session.sessionId}
        <Composer
          running={$session.running !== null}
          onSend={send}
          onCancel={cancel}
          onError={(message) => (uiError = message)}
          onAttach={(paths) =>
            attachFiles($session.workspace, $session.sessionId, paths)}
          initialDraft={prefillDraft}
          history={promptHistory}
        />
      {/key}

      <StatusBar
        {status}
        rigTitle={status ? $rigTitle(status.rig, status.rig) : ""}
        rigs={rigs}
        rigSelectable={emptyConversation}
        {presets}
        {activePreset}
        busy={$session.running !== null}
        onRigChange={chooseRig}
        onPermission={changePermission}
        onPreset={choosePreset}
        onSavePreset={storePreset}
        onManagePresets={onSettings}
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
