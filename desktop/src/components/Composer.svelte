<script lang="ts">
  import { t } from "../lib/i18n";

  let {
    running,
    onSend,
    onCancel,
    onError,
  }: {
    running: boolean;
    onSend: (text: string) => Promise<void>;
    onCancel: () => void;
    onError: (message: string) => void;
  } = $props();

  let draft = $state("");

  async function send(): Promise<void> {
    const text = draft.trim();
    if (!text || running) return;
    try {
      await onSend(text);
      draft = "";
    } catch (error) {
      onError(errText(error));
    }
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void send();
    }
  }

  function errText(error: unknown): string {
    if (typeof error === "string") return error;
    if (error instanceof Error) return error.message;
    return String(error);
  }
</script>

<div class="composer">
  <textarea
    bind:value={draft}
    onkeydown={onKeydown}
    placeholder={$t("app.placeholder")}
    rows="2"
  ></textarea>
  {#if running}
    <button onclick={onCancel}>{$t("app.cancel")}</button>
  {:else}
    <button onclick={send} disabled={!draft.trim()}>{$t("app.send")}</button>
  {/if}
</div>
