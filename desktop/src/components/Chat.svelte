<script lang="ts">
  import {
    archiveSession,
    cancelTurn,
    deleteSession,
    listSessions,
    newSession,
    openSession,
    renameSession,
    respondInteraction,
    sendMessage,
    undoLastTurn,
  } from "../lib/api";
  import { dispatch, session } from "../lib/session-store";
  import { t } from "../lib/i18n";
  import { baseName } from "../lib/paths";
  import AppHeader from "./AppHeader.svelte";
  import Composer from "./Composer.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import PermissionCard from "./PermissionCard.svelte";
  import Sidebar from "./Sidebar.svelte";
  import Transcript from "./Transcript.svelte";

  let { onSettings }: { onSettings: () => void } = $props();

  let uiError = $state<string | null>(null);
  let answering = $state<string | null>(null);
  let toast = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | null = null;
  let confirmUndo = $state(false);
  let undoDone = $state(false);

  const MUTATING_TOOLS = new Set([
    "write_file",
    "edit_file",
    "multi_edit",
    "move_file",
    "delete_file",
    "make_dir",
    "restore_file",
    "shell",
  ]);

  let latestTurn = $derived(
    $session.turns.length > 0
      ? $session.turns[$session.turns.length - 1]
      : null,
  );
  let canUndo = $derived(
    !$session.running &&
      !undoDone &&
      latestTurn !== null &&
      latestTurn.tools.some((tool) => MUTATING_TOOLS.has(tool.name)),
  );

  $effect(() => {
    $session.sessionId;
    $session.turns.length;
    undoDone = false;
    confirmUndo = false;
  });

  let currentTitle = $derived(
    $session.sessions.find((item) => item.id === $session.sessionId)?.title ||
      $t("sidebar.untitled"),
  );
  let projectName = $derived(baseName($session.workspace) || $session.workspace);
  let currentArchived = $derived(
    $session.sessions.find((item) => item.id === $session.sessionId)
      ?.archived ?? false,
  );

  function notify(message: string): void {
    toast = message;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toast = null;
      toastTimer = null;
    }, 3000);
  }

  async function undo(): Promise<void> {
    confirmUndo = false;
    uiError = null;
    try {
      await undoLastTurn();
      undoDone = true;
      notify($t("app.undone"));
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
    try {
      const view = await openSession($session.workspace, id);
      dispatch({ type: "session-opened", view });
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
      notify(
        wasCurrent
          ? $t("sidebar.deletedCurrent")
          : $t("sidebar.deleted"),
      );
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
      notify(
        archived
          ? $t("sidebar.archivedToast")
          : $t("sidebar.unarchivedToast"),
      );
    } catch (error) {
      uiError = errText(error);
    }
  }

  async function unarchiveCurrent(): Promise<void> {
    if (!$session.sessionId) return;
    await archiveConversation($session.sessionId, false);
  }

  async function send(text: string): Promise<void> {
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

  function dismissError(): void {
    uiError = null;
    dispatch({ type: "clear-error" });
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
      {projectName}
      {canUndo}
      onUndo={() => (confirmUndo = true)}
    />

    <footer>
      {#if toast}
        <div class="toast" role="status">{toast}</div>
      {/if}

      {#if $session.pendingInteraction}
        <PermissionCard
          interaction={$session.pendingInteraction}
          {answering}
          onAnswer={answer}
        />
      {/if}

      {#if uiError || $session.lastError}
        <div class="banner">
          <span>{uiError ?? $session.lastError}</span>
          <button class="icon" onclick={dismissError} title={$t("app.dismiss")}>
            ×
          </button>
        </div>
      {/if}

      {#if currentArchived}
        <div class="archived-notice" role="status">
          <span>{$t("app.archivedNotice")}</span>
          <button onclick={unarchiveCurrent}>
            {$t("sidebar.unarchive")}
          </button>
        </div>
      {:else}
        {#key $session.sessionId}
          <Composer
            running={$session.running !== null}
            onSend={send}
            onCancel={cancel}
            onError={(message) => (uiError = message)}
            showStarters={$session.turns.length === 0 && !$session.running}
          />
        {/key}
      {/if}
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
