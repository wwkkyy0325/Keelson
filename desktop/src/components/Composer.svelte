<script lang="ts">
  import { onMount, untrack } from "svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { t } from "../lib/i18n";
  import type { Attachment } from "../lib/protocol";

  let {
    running,
    onSend,
    onCancel,
    onError,
    onAttach,
    initialDraft = "",
    history = [],
  }: {
    running: boolean;
    onSend: (text: string) => Promise<void>;
    onCancel: () => Promise<void> | void;
    onError: (message: string) => void;
    /// Copies picked paths into the conversation folder and returns them.
    onAttach?: (paths: string[]) => Promise<Attachment[]>;
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
  /// A file chosen for the next message. Nothing is copied until the send:
  /// dropping a file is only a note about what to hand over.
  type Pending = { name: string; source: string };
  let pending = $state<Pending[]>([]);

  function baseName(path: string): string {
    return path.split(/[\\/]/).pop() || path;
  }

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
    // Native drops anywhere in the window land here; the composer is the only
    // place they can go.
    let disposed = false;
    let unlisten: UnlistenFn | null = null;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "drop") addPaths(event.payload.paths);
      })
      .then((off) => {
        if (disposed) off();
        else unlisten = off;
      });
    return () => {
      disposed = true;
      window.removeEventListener("resize", onResize);
      unlisten?.();
    };
  });

  /// A drop or a pick only records the source; the same path twice is one
  /// entry. The copy is made when the message is sent.
  function addPaths(paths: string[]): void {
    const known = new Set(pending.map((file) => file.source));
    const added: Pending[] = [];
    for (const path of paths) {
      if (known.has(path)) continue;
      known.add(path);
      added.push({ name: baseName(path), source: path });
    }
    if (added.length > 0) pending = [...pending, ...added];
  }

  async function pick(): Promise<void> {
    try {
      const picked = await openDialog({ multiple: true, title: $t("app.attach") });
      if (!picked) return;
      addPaths(Array.isArray(picked) ? picked : [picked]);
    } catch (error) {
      onError(errText(error));
    }
  }

  async function pickFolder(): Promise<void> {
    try {
      const picked = await openDialog({
        directory: true,
        multiple: true,
        title: $t("app.attachFolder"),
      });
      if (!picked) return;
      addPaths(Array.isArray(picked) ? picked : [picked]);
    } catch (error) {
      onError(errText(error));
    }
  }

  function removeAttachment(source: string): void {
    pending = pending.filter((file) => file.source !== source);
  }

  /// The copied paths travel in the message itself, so the transcript and the
  /// model see the same list after a restart.
  function compose(text: string, files: Attachment[]): string {
    if (files.length === 0) return text;
    const list = files.map((file) => `- ${file.path}`).join("\n");
    const block = `${$t("app.attachmentsHeader")}\n${list}`;
    return text ? `${text}\n\n${block}` : block;
  }

  async function send(): Promise<void> {
    const text = draft.trim();
    if ((!text && pending.length === 0) || running || sending) return;
    sending = true;
    // The box empties on the click; the turn row appears from the event. A
    // rejected send puts the text and the files back rather than losing them.
    const sent = pending;
    draft = "";
    pending = [];
    cursor = -1;
    savedDraft = "";
    try {
      const files = sent.length > 0 && onAttach
        ? await onAttach(sent.map((file) => file.source))
        : [];
      await onSend(compose(text, files));
    } catch (error) {
      if (!draft.trim()) draft = text;
      pending = sent;
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
  <div class="composer-main">
    {#if pending.length > 0}
      <div class="attachments">
        {#each pending as file (file.source)}
          <span class="attachment" title={file.source}>
            <span class="attachment-name">{file.name}</span>
            <button
              class="attachment-remove"
              aria-label={$t("app.removeAttachment")}
              onclick={() => removeAttachment(file.source)}
            >
              ×
            </button>
          </span>
        {/each}
      </div>
    {/if}
    <textarea
      bind:this={textarea}
      bind:value={draft}
      onkeydown={onKeydown}
      placeholder={$t("app.placeholder")}
      rows="2"
    ></textarea>
  </div>
  <div class="composer-actions">
    <button
      class="icon attach"
      onclick={pick}
      disabled={!onAttach}
      aria-label={$t("app.attach")}
      title={$t("app.attach")}
    >
      <svg
        viewBox="0 0 24 24"
        width="15"
        height="15"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
      >
        <path
          d="M21 12.5 12.2 21.3a5.5 5.5 0 0 1-7.8-7.8l8.6-8.6a3.7 3.7 0 0 1 5.2 5.2l-8.6 8.6a1.8 1.8 0 0 1-2.6-2.6l7.9-7.9"
        />
      </svg>
    </button>
    <button
      class="icon attach"
      onclick={pickFolder}
      disabled={!onAttach}
      aria-label={$t("app.attachFolder")}
      title={$t("app.attachFolder")}
    >
      <svg
        viewBox="0 0 24 24"
        width="15"
        height="15"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" />
      </svg>
    </button>
    {#if running}
      <button onclick={cancel} disabled={cancelling}>
        {cancelling ? $t("app.cancelling") : $t("app.cancel")}
      </button>
    {:else}
      <button
        onclick={send}
        disabled={(!draft.trim() && pending.length === 0) || sending}
      >
        {$t("app.send")}
      </button>
    {/if}
  </div>
</div>
