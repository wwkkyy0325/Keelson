<script lang="ts">
  import { t } from "../lib/i18n";
  import type { SessionStatus } from "../lib/protocol";

  let {
    status,
    rigTitle,
    busy = false,
    onSettings,
    onPermission,
  }: {
    status: SessionStatus | null;
    rigTitle: string;
    busy?: boolean;
    onSettings: () => void;
    onPermission: (level: string) => Promise<void>;
  } = $props();

  let permissionOpen = $state(false);
  let contextOpen = $state(false);

  let used = $derived(
    status && status.recent.length > 0
      ? status.recent[status.recent.length - 1].used
      : 0,
  );
  // The model window is the only bound a user needs to see; the working-set
  // budget is a kernel detail that moves as the ladder escalates.
  let window = $derived(status?.contextWindow ?? status?.contextBudget ?? 0);
  let percent = $derived(
    window > 0 ? Math.round((used / window) * 100) : 0,
  );

  function label(level: string | undefined): string {
    return level === "full_auto"
      ? $t("status.fullAuto")
      : $t("status.workspaceAuto");
  }

  async function choose(level: string): Promise<void> {
    permissionOpen = false;
    if (!status || status.permission === level) return;
    await onPermission(level);
  }

  function compact(count: number): string {
    if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
    if (count >= 100_000) return `${Math.round(count / 1000)}k`;
    if (count >= 1000) return `${(count / 1000).toFixed(1)}k`;
    return String(count);
  }
</script>

{#if status}
  <div class="status-bar">
    <div class="status-left">
      <div class="status-menu">
        <button
          class="status-item"
          onclick={() => (permissionOpen = !permissionOpen)}
          disabled={busy}
          aria-haspopup="menu"
          aria-expanded={permissionOpen}
          title={$t("status.permission")}
        >
          {label(status.permission)}
          <span class="caret">▾</span>
        </button>
        {#if permissionOpen}
          <button
            class="status-backdrop"
            aria-label={$t("app.dismiss")}
            onclick={() => (permissionOpen = false)}
          ></button>
          <div class="status-popover" role="menu">
            <button
              role="menuitem"
              class:active={status.permission === "workspace_auto"}
              onclick={() => choose("workspace_auto")}
            >
              <strong>{$t("status.workspaceAuto")}</strong>
              <span>{$t("status.workspaceAutoHint")}</span>
            </button>
            <button
              role="menuitem"
              class:active={status.permission === "full_auto"}
              onclick={() => choose("full_auto")}
            >
              <strong>{$t("status.fullAuto")}</strong>
              <span>{$t("status.fullAutoHint")}</span>
            </button>
          </div>
        {/if}
      </div>

      <button
        class="status-item"
        onclick={onSettings}
        title={$t("status.settingsHint")}
      >
        {rigTitle} · {status.providerName || status.provider} {status.model}
      </button>
    </div>

    <div class="status-menu">
      <button
        class="status-item"
        onclick={() => (contextOpen = !contextOpen)}
        aria-haspopup="true"
        aria-expanded={contextOpen}
        title={$t("status.context")}
      >
        <span class="status-num">{compact(used)} / {compact(window)}</span>
        <span class="status-pct">{percent}%</span>
        <span class="caret">▾</span>
      </button>
      {#if contextOpen}
        <button
          class="status-backdrop"
          aria-label={$t("app.dismiss")}
          onclick={() => (contextOpen = false)}
        ></button>
        <div class="status-popover right">
          {#if status.recent.length === 0}
            <p class="status-empty">{$t("status.contextEmpty")}</p>
          {:else}
            {#each status.recent as point}
              <div class="status-row">
                <span>{$t("status.contextTurn", { seq: point.seq })}</span>
                <span class="status-num">
                  {compact(point.used)} / {compact(window)}
                </span>
              </div>
            {/each}
          {/if}
          <p class="status-hint">
            {$t("status.contextWindow", { window: compact(window) })}
          </p>
        </div>
      {/if}
    </div>
  </div>
{/if}
