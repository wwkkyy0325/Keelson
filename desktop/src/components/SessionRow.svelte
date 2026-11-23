<script lang="ts">
  import { tick } from "svelte";
  import { t } from "../lib/i18n";
  import type { SessionMeta } from "../lib/protocol";

  let {
    item,
    active,
    archived,
    running,
    onSelect,
    onRename,
    onArchive,
    onRequestDelete,
  }: {
    item: SessionMeta;
    active: boolean;
    archived: boolean;
    running: boolean;
    onSelect: (id: string) => void;
    onRename: (id: string, title: string) => void;
    onArchive: (id: string, archived: boolean) => void;
    onRequestDelete: (item: SessionMeta) => void;
  } = $props();

  let renaming = $state(false);
  let draft = $state("");
  let input = $state<HTMLInputElement | null>(null);

  /// Only the conversation on screen can be running, and only its own actions
  /// have to wait for it. Another conversation's row stays usable: a turn in
  /// one conversation must not lock the sidebar.
  let busy = $derived(active && running);

  function startRename(): void {
    draft = item.title;
    renaming = true;
    tick().then(() => {
      input?.focus();
      input?.select();
    });
  }

  function commitRename(): void {
    if (!renaming) return;
    renaming = false;
    const next = draft.trim();
    if (next !== item.title) onRename(item.id, next);
  }

  function cancelRename(): void {
    renaming = false;
  }

  function onRenameKey(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      event.preventDefault();
      commitRename();
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancelRename();
    }
  }

  function toggleArchive(): void {
    if (busy) return;
    onArchive(item.id, !archived);
  }

  function requestDelete(): void {
    if (busy) return;
    onRequestDelete(item);
  }

  function when(ms: number): string {
    const date = new Date(ms);
    const now = new Date();
    if (date.toDateString() === now.toDateString()) {
      return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    }
    return date.toLocaleDateString([], { month: "2-digit", day: "2-digit" });
  }
</script>

{#if renaming}
  <div class="session-row editing">
    <input
      class="session-rename"
      bind:this={input}
      bind:value={draft}
      onkeydown={onRenameKey}
      onblur={commitRename}
      aria-label={$t("sidebar.rename")}
      maxlength="48"
    />
  </div>
{:else}
  <div class="session-row" class:active class:archived>
    <button
      class="session-main"
      onclick={() => onSelect(item.id)}
      aria-current={active ? "true" : undefined}
    >
      <span class="session-title">
        {item.title || $t("sidebar.untitled")}
      </span>
      <span class="session-time">{when(item.updatedAt)}</span>
    </button>
    <div class="session-actions">
      <button
        class="session-action"
        onclick={startRename}
        title={$t("sidebar.rename")}
        aria-label={$t("sidebar.rename")}
      >
        <svg
          width="12"
          height="12"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M12 20h9" />
          <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z" />
        </svg>
      </button>
      <button
        class="session-action"
        onclick={toggleArchive}
        disabled={busy}
        title={archived
          ? $t("sidebar.unarchive")
          : `${$t("sidebar.archive")} · ${$t("sidebar.archiveHint")}`}
        aria-label={archived ? $t("sidebar.unarchive") : $t("sidebar.archive")}
      >
        {#if archived}
          <svg
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M5 8v11a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V8" />
            <path d="M9 11l3-3 3 3" />
            <path d="M12 8v5" />
          </svg>
        {:else}
          <svg
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <rect x="3" y="4" width="18" height="4" rx="1" />
            <path d="M5 8v11a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V8" />
            <path d="M10 12h4" />
          </svg>
        {/if}
      </button>
      {#if archived}
        <button
          class="session-action delete"
          onclick={requestDelete}
          disabled={busy}
          title={$t("sidebar.delete")}
          aria-label={$t("sidebar.delete")}
        >
          ×
        </button>
      {/if}
    </div>
  </div>
{/if}
