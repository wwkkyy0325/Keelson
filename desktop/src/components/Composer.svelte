<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { t } from "../lib/i18n";

  let {
    running,
    onSend,
    onCancel,
    onError,
    showStarters = false,
    initialDraft = "",
  }: {
    running: boolean;
    onSend: (text: string) => Promise<void>;
    onCancel: () => void;
    onError: (message: string) => void;
    // An empty conversation offers a few things to start from, so nobody has
    // to decide what to say first.
    showStarters?: boolean;
    /// Text a branch opens with, so the user can edit the turn they moved from.
    initialDraft?: string;
  } = $props();

  // The composer grows with its content and stops at 40% of the window, where
  // it starts to scroll. There is no handle and no remembered height: the
  // size is always what the draft needs.
  const MAX_SHARE = 0.4;
  const MIN_HEIGHT = 96;

  let draft = $state(untrack(() => initialDraft));
  let textarea = $state<HTMLTextAreaElement | null>(null);

  function autosize(): void {
    if (!textarea) return;
    textarea.style.height = "auto";
    const max = Math.max(MIN_HEIGHT, Math.round(window.innerHeight * MAX_SHARE));
    textarea.style.height = `${Math.min(textarea.scrollHeight, max)}px`;
    textarea.style.overflowY = textarea.scrollHeight > max ? "auto" : "hidden";
  }

  // Runs after the DOM has the new value, and again when a send clears it.
  $effect(() => {
    draft;
    autosize();
  });

  onMount(() => {
    const onResize = (): void => autosize();
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  });

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

  function useStarter(text: string): void {
    draft = text;
    textarea?.focus();
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

<div class="composer-block">
  {#if showStarters && !draft.trim()}
    <div class="starters">
      <button type="button" onclick={() => useStarter($t("app.starterFixTests"))}>
        {$t("app.starterFixTests")}
      </button>
      <button type="button" onclick={() => useStarter($t("app.starterRunProject"))}>
        {$t("app.starterRunProject")}
      </button>
      <button type="button" onclick={() => useStarter($t("app.starterExplain"))}>
        {$t("app.starterExplain")}
      </button>
      <button type="button" onclick={() => useStarter($t("app.starterAddFeature"))}>
        {$t("app.starterAddFeature")}
      </button>
    </div>
  {/if}
  <div class="composer">
    <textarea
      bind:this={textarea}
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
</div>
