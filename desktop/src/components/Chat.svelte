<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { cancelTurn, respondInteraction, sendMessage } from "../lib/api";
  import { dispatch, session } from "../lib/session-store";
  import { t } from "../lib/i18n";
  import AppHeader from "./AppHeader.svelte";
  import Composer from "./Composer.svelte";
  import PermissionCard from "./PermissionCard.svelte";
  import Transcript from "./Transcript.svelte";

  let {
    onConnect,
    onSettings,
  }: {
    onConnect: (workspace?: string) => Promise<string | null>;
    onSettings: () => void;
  } = $props();

  let uiError = $state<string | null>(null);
  let answering = $state<string | null>(null);

  async function changeWorkspace(): Promise<void> {
    const picked = await openDialog({
      directory: true,
      multiple: false,
      defaultPath: $session.workspace || undefined,
    });
    if (typeof picked === "string" && picked !== $session.workspace) {
      const error = await onConnect(picked);
      if (error) uiError = error;
    }
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
  <AppHeader
    workspace={$session.workspace}
    onWorkspace={changeWorkspace}
    {onSettings}
  />

  <Transcript onError={(message) => (uiError = message)} />

  <footer>
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

    <Composer
      running={$session.running !== null}
      onSend={send}
      onCancel={cancel}
      onError={(message) => (uiError = message)}
    />
  </footer>
</div>
