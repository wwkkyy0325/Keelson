<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { t } from "../lib/i18n";

  let {
    running,
    onSend,
    onCancel,
    onError,
    initialDraft = "",
    history = [],
  }: {
    running: boolean;
    onSend: (text: string) => Promise<void>;
    onCancel: () => Promise<void> | void;
    onError: (message: string) => void;
    /// Text a branch opens with, so the user can edit the turn they moved from.
    initialDraft?: string;
    /// Earlier user inputs, oldest first: ArrowUp/ArrowDown walk them.
    history?: string[];
  } = $props();

  /// -1 while the draft is being edited; otherwise the index into `history`.
  let cursor = -1;
  let savedDraft = "";

  // The composer grows with its content and stops at 40% of the window, where
  // it starts to scroll. There is no handle and no remembered height: the
  // size is always what the draft needs.
  const MAX_SHARE = 0.4;
  const MIN_HEIGHT = 96;

  let draft = $state(untrack(() => initialDraft));
  let textarea = $state<HTMLTextAreaElement | null>(null);
  /// A send is in flight. The turn-started event arrives a moment later, so
  /// this closes the gap in which a second Enter could reach the backend.
  let sending = $state(false);
  let cancelling = $state(false);

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
    if (!text || running || sending) return;
    sending = true;
    try {
      await onSend(text);
      draft = "";
      cursor = -1;
      savedDraft = "";
    } catch (error) {
      onError(errText(error));
    } finally {
      sending = false;
    }
  }

  /// The kernel stops between steps, so the button reports the click at once
  /// and the turn ends a moment later.
  async function cancel(): Promise<void> {
    if (cancelling) return;
    cancelling = true;
    try {
      await onCancel();
    } catch (error) {
      onError(errText(error));
      cancelling = false;
    }
  }

  // The button is only shown while a turn runs; when it ends, the label
  // resets for the next one.
  $effect(() => {
    if (!running) cancelling = false;
  });

  /// Arrow history behaves like a shell: the draft is kept aside while the
  /// earlier inputs are walked, and ArrowDown past the newest brings it back.
  function recall(direction: -1 | 1): void {
    if (cursor === -1) {
      savedDraft = draft;
      cursor = history.length;
    }
    cursor = Math.min(history.length, Math.max(0, cursor + direction));
    if (cursor >= history.length) {
      cursor = -1;
      draft = savedDraft;
      return;
    }
    draft = history[cursor];
    requestAnimationFrame(() => {
      const end = draft.length;
      textarea?.setSelectionRange(end, end);
    });
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.isComposing) return;
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void send();
      return;
    }
    if (event.key === "ArrowUp" && history.length > 0 && canRecall()) {
      event.preventDefault();
      recall(-1);
      return;
    }
    if (event.key === "ArrowDown" && cursor >= 0) {
      event.preventDefault();
      recall(1);
    }
  }

  /// The first line with an empty draft, or anywhere while already browsing.
  function canRecall(): boolean {
    if (cursor >= 0 || draft.length === 0) return true;
    return textarea?.selectionStart === 0 && textarea?.selectionEnd === 0;
  }

  function errText(error: unknown): string {
    if (typeof error === "string") return error;
    if (error instanceof Error) return error.message;
    return String(error);
  }
</script>

<div class="composer">
  <textarea
    bind:this={textarea}
    bind:value={draft}
    onkeydown={onKeydown}
    placeholder={$t("app.placeholder")}
    rows="2"
  ></textarea>
  {#if running}
    <button onclick={cancel} disabled={cancelling}>
      {cancelling ? $t("app.cancelling") : $t("app.cancel")}
    </button>
  {:else}
    <button onclick={send} disabled={!draft.trim() || sending}>
      {$t("app.send")}
    </button>
  {/if}
</div>
