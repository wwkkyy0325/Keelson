<script lang="ts">
  import { onDestroy, untrack } from "svelte";
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
  import { perf } from "../lib/perf";
  import { ACCOUNT_EVERY_MS, reportTurn } from "../lib/report";
  import { visible } from "../lib/visible";
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
      const tasks = await perf.timeAsync("background.sync", () =>
        backgroundTasks(),
      );
      // A switch mid-request must not paint the previous session's rows.
      if ($session.epoch !== epoch) return;
      dispatch({
        type: "background-synced",
        tasks: tasks.map((task) => ({
          id: task.id,
          program: task.program,
          command: task.command,
          ok: task.running ? null : task.exitCode === 0,
          durationMs: task.durationMs,
          outputPath: task.outputPath,
          stopped: task.stopped,
        })),
      });
    } catch {
      // A transient failure must not freeze the row: the next poll retries.
    }
  }

  /// A running row's clock moves every second, so the poll follows it. A row
  /// that already exited cannot change until the engine drains it at the next
  /// turn, so an idle strip polls once every five ticks and clears itself.
  let backgroundTick = 0;

  function pollBackground(): void {
    if (backgroundTimer !== null) return;
    backgroundTick = 0;
    // Read once here, not in the effect that arms the poll: `syncBackground`
    // brackets its request with the epoch, and a store read inside a tracked
    // scope makes that scope depend on every dispatch (ADR 0072).
    void syncBackground();
    backgroundTimer = setInterval(() => {
      backgroundTick += 1;
      perf.count("background.tick");
      const running = $session.background.some((task) => task.ok === null);
      if (running || backgroundTick % 5 === 0) void syncBackground();
    }, 1000);
  }

  function stopPoll(): void {
    if (backgroundTimer !== null) {
      clearInterval(backgroundTimer);
      backgroundTimer = null;
    }
  }

  // A background command outlives the view. On every session open, reconcile
  // first: a task started before a switch or a reconnect has to reappear, and
  // its start event will never be replayed.
  //
  // The epoch guard is not an optimisation (ADR 0072). This effect read the
  // store, so every dispatch re-ran it, and it dispatched on every run — the
  // same loop the poll below was in, and both had to be closed. A re-run of
  // this effect is not a new conversation; only an epoch that changed is.
  let reconciledEpoch = -1;
  $effect(() => {
    const epoch = $session.epoch;
    untrack(() => {
      if (reconciledEpoch === epoch) return;
      reconciledEpoch = epoch;
      void syncBackground();
    });
  });

  // The poll is armed by a boolean, and nothing the poll does is tracked. Both
  // halves are load-bearing (ADR 0072). The body used to call `pollBackground`
  // directly, which reads the store synchronously; the effect therefore
  // re-ran on every dispatch, and its cleanup cleared the interval it had just
  // armed, so the next run armed it again and polled again. With one running
  // row on screen that was 1999 arms and 2001 host calls in four seconds, and
  // the renderer at 117% (measured in WebKit, real component).
  $effect(() => {
    const wanted = backgroundVisible && $visible;
    untrack(() => {
      if (wanted) {
        pollBackground();
      } else {
        // A new session, a drained task, or a window nobody can see: the rows
        // are not on screen, so the poll goes quiet. The kernel keeps the
        // command running; the rows are read again when the window returns.
        stopPoll();
      }
    });
  });

  // The conversation closed: no more reads against a gone engine. This effect
  // has no tracked dependency, so it runs once and cleans up only at destroy.
  $effect(() => () => stopPoll());

  /// Every turn leaves an account on disk (lib/report.ts, ADR 0071). The effect
  /// tracks the sequence number alone: reading the running card itself would
  /// re-run it on every streamed delta, which is the thing being measured.
  let runningSeq = $derived($session.running?.seq ?? -1);
  let accountTimer: ReturnType<typeof setInterval> | null = null;
  let accountedSeq = -1;
  let accountedWorkspace = "";

  function stopAccounting(): void {
    if (accountTimer !== null) {
      clearInterval(accountTimer);
      accountTimer = null;
    }
    if (accountedSeq >= 0) {
      const seq = accountedSeq;
      accountedSeq = -1;
      void reportTurn(accountedWorkspace, seq, "end");
    }
  }

  $effect(() => {
    const seq = runningSeq;
    const workspace = $session.workspace;
    untrack(() => {
      if (seq < 0) {
        stopAccounting();
        return;
      }
      if (accountedSeq !== seq) {
        accountedSeq = seq;
        accountedWorkspace = workspace;
        void reportTurn(workspace, seq, "start");
      }
      if (accountTimer === null) {
        // The account of a turn in flight is written over itself, so a spike is
        // bracketed by the sample that follows it without growing a file per
        // tick.
        accountTimer = setInterval(() => {
          void reportTurn(accountedWorkspace, seq, "during");
        }, ACCOUNT_EVERY_MS);
      }
    });
  });

  onDestroy(() => {
    if (accountTimer !== null) {
      clearInterval(accountTimer);
      accountTimer = null;
    }
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
    // The answer names the conversation it describes: if the window moved on
    // while it was in flight, or the same id was reopened, it is stale and must
    // not paint its rig or context over the current conversation.
    const id = $session.sessionId;
    const epoch = $session.epoch;
    try {
      const next = await sessionStatus();
      if (next.sessionId !== id || $session.epoch !== epoch) return;
      status = next;
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

  /// The rig the open conversation actually runs. It comes back with the view,
  /// which is the only place that knows which engine is behind the window; the
  /// status fetch is a fallback, because it answers for whichever conversation
  /// the backend had current when it was asked.
  let rigName = $derived($session.rig || status?.rig || "");

  async function chooseRig(name: string): Promise<void> {
    if (!emptyConversation || !$session.sessionId || rigName === name) {
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
    uiError = null;
    try {
      const view = await openSession(path, null);
      dispatch({ type: "session-opened", view });
    } catch (error) {
      uiError = errText(error);
    }
  }

  async function newConversation(): Promise<void> {
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
    // Switching is allowed while a turn runs: the turn belongs to its own
    // conversation and keeps streaming there, and its events are tagged so they
    // never paint into this one.
    if (id === $session.sessionId) return;
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
    if (id === $session.sessionId && $session.running) return;
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
    if (id === $session.sessionId && $session.running) return;
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
        rigTitle={rigName ? $rigTitle(rigName, rigName) : ""}
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
