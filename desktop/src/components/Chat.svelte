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
  } from "../lib/api";
  import { dispatch, session } from "../lib/session-store";
  import { t } from "../lib/i18n";
  import { baseName } from "../lib/paths";
  import AppHeader from "./AppHeader.svelte";
  import Composer from "./Composer.svelte";
  import PermissionCard from "./PermissionCard.svelte";
  import Sidebar from "./Sidebar.svelte";
  import Transcript from "./Transcript.svelte";

  let { onSettings }: { onSettings: () => void } = $props();

  let uiError = $state<string | null>(null);
  let answering = $state<string | null>(null);
  let toast = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

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
          />
        {/key}
      {/if}
    </footer>
  </div>
</div>
