<script lang="ts">
  import { onMount } from "svelte";
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

  const MIN_HEIGHT = 64;
  const HEIGHT_KEY = "keelson.composer.height";

  let draft = $state("");
  let height = $state<number | null>(null);
  let dragging = $state(false);
  let startY = 0;
  let startHeight = 0;
  let textarea = $state<HTMLTextAreaElement | null>(null);

  onMount(() => {
    const saved = Number(window.localStorage.getItem(HEIGHT_KEY));
    if (Number.isFinite(saved) && saved >= MIN_HEIGHT) height = saved;
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

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void send();
    }
  }

  function maxHeight(): number {
    return Math.max(MIN_HEIGHT, Math.round(window.innerHeight * 0.6));
  }

  function setHeight(next: number): void {
    height = Math.min(maxHeight(), Math.max(MIN_HEIGHT, Math.round(next)));
    window.localStorage.setItem(HEIGHT_KEY, String(height));
  }

  // The composer sits at the bottom of the window, so it grows upward: the
  // grip is on its top edge and dragging up makes it taller. It is a button
  // so it is focusable and announced, and the arrow keys resize it too.
  function startResize(event: PointerEvent): void {
    if (event.button !== 0 || !textarea) return;
    dragging = true;
    startY = event.clientY;
    startHeight = height ?? textarea.offsetHeight;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    event.preventDefault();
  }

  function drag(event: PointerEvent): void {
    if (!dragging) return;
    setHeight(startHeight - (event.clientY - startY));
  }

  function stopResize(event: PointerEvent): void {
    if (!dragging) return;
    dragging = false;
    (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
  }

  function resizeWithKeys(event: KeyboardEvent): void {
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    event.preventDefault();
    const current = height ?? textarea?.offsetHeight ?? MIN_HEIGHT;
    setHeight(current + (event.key === "ArrowUp" ? 24 : -24));
  }

  function errText(error: unknown): string {
    if (typeof error === "string") return error;
    if (error instanceof Error) return error.message;
    return String(error);
  }
</script>

<div class="composer">
  <button
    type="button"
    class="composer-grip"
    class:dragging
    aria-label={$t("app.resizeComposer")}
    onpointerdown={startResize}
    onpointermove={drag}
    onpointerup={stopResize}
    onpointercancel={stopResize}
    onkeydown={resizeWithKeys}
  ></button>
  <div class="composer-row">
    <textarea
      bind:this={textarea}
      bind:value={draft}
      onkeydown={onKeydown}
      placeholder={$t("app.placeholder")}
      rows="2"
      style:height={height ? `${height}px` : null}
    ></textarea>
    {#if running}
      <button onclick={onCancel}>{$t("app.cancel")}</button>
    {:else}
      <button onclick={send} disabled={!draft.trim()}>{$t("app.send")}</button>
    {/if}
  </div>
</div>
