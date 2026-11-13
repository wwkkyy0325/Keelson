<script lang="ts">
  import { rigLabel, t } from "../lib/i18n";
  import type { Preset, RigOption, SessionStatus } from "../lib/protocol";

  let {
    status,
    rigTitle,
    rigs = [],
    rigSelectable = false,
    presets = [],
    activePreset = null,
    busy = false,
    onRigChange,
    onPermission,
    onPreset,
    onSavePreset,
    onManagePresets,
  }: {
    status: SessionStatus | null;
    rigTitle: string;
    rigs?: RigOption[];
    rigSelectable?: boolean;
    presets?: Preset[];
    activePreset?: string | null;
    busy?: boolean;
    onRigChange: (name: string) => Promise<void>;
    onPermission: (level: string) => Promise<void>;
    onPreset: (id: string) => Promise<void>;
    onSavePreset: (name: string) => Promise<void>;
    onManagePresets: () => void;
  } = $props();

  let permissionOpen = $state(false);
  let contextOpen = $state(false);
  let rigOpen = $state(false);
  let presetOpen = $state(false);
  let presetName = $state("");

  let activePresetName = $derived(
    presets.find((preset) => preset.id === activePreset)?.name ?? "",
  );

  async function pickPreset(id: string): Promise<void> {
    presetOpen = false;
    await onPreset(id);
  }

  async function savePreset(): Promise<void> {
    const name = presetName.trim();
    presetName = "";
    presetOpen = false;
    await onSavePreset(name);
  }

  /// The localized title of the running rig, or its file name if the list is
  /// stale.
  function rigLabelOf(name: string): string {
    const found = rigs.find((rig) => rig.name === name);
    return found ? $rigLabel(found.name, found).title : name;
  }

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

  /// The effective level is what the chip reports; the menu marks and
  /// persists the base level. A live grant can make the two differ.
  let granted = $derived(
    status !== null &&
      status.permissionEffective === "full_auto" &&
      status.permission !== "full_auto",
  );

  async function choose(level: string): Promise<void> {
    permissionOpen = false;
    // Choosing the base level again is a no-op only when no grant is live:
    // with one, the same click is how the user drops it.
    if (
      !status ||
      (status.permission === level &&
        status.permissionEffective === status.permission)
    ) {
      return;
    }
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
          onclick={() => (presetOpen = !presetOpen)}
          disabled={busy}
          aria-haspopup="menu"
          aria-expanded={presetOpen}
          title={$t("status.preset")}
        >
          {activePresetName || $t("status.preset")}
          <span class="caret">▾</span>
        </button>
        {#if presetOpen}
          <button
            class="status-backdrop"
            aria-label={$t("app.dismiss")}
            onclick={() => (presetOpen = false)}
          ></button>
          <div class="status-popover" role="menu">
            {#each presets as preset (preset.id)}
              <button
                role="menuitem"
                class:active={preset.id === activePreset}
                onclick={() => pickPreset(preset.id)}
              >
                <strong>{preset.name}</strong>
                <span>
                  {preset.rig} · {preset.providerName || preset.provider}
                  {preset.model}
                </span>
              </button>
            {/each}
            {#if presets.length > 0}<div class="preset-sep"></div>{/if}
            <div class="preset-save">
              <input
                bind:value={presetName}
                placeholder={$t("status.presetName")}
                spellcheck="false"
              />
              <button class="ghost" onclick={savePreset}>
                {$t("status.presetSave")}
              </button>
            </div>
            <button
              class="status-row"
              onclick={() => {
                presetOpen = false;
                onManagePresets();
              }}
            >
              {$t("status.presetManage")}
            </button>
          </div>
        {/if}
      </div>

      <div class="status-menu">
        <button
          class="status-item"
          onclick={() => (permissionOpen = !permissionOpen)}
          disabled={busy}
          aria-haspopup="menu"
          aria-expanded={permissionOpen}
          title={$t("status.permission")}
        >
          {granted ? $t("status.fullAutoTemporary") : label(status.permission)}
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

      {#if rigSelectable && rigs.length > 0}
        <div class="status-menu">
          <button
            class="status-item"
            onclick={() => (rigOpen = !rigOpen)}
            aria-haspopup="menu"
            aria-expanded={rigOpen}
            title={$t("status.rigChoose")}
          >
            {rigLabelOf(status.rig)}
            <span class="caret">▾</span>
          </button>
          {#if rigOpen}
            <button
              class="status-backdrop"
              aria-label={$t("app.dismiss")}
              onclick={() => (rigOpen = false)}
            ></button>
            <div class="status-popover" role="menu">
              {#each rigs as rig (rig.name)}
                <button
                  role="menuitem"
                  class:active={rig.name === status.rig}
                  onclick={() => {
                    rigOpen = false;
                    void onRigChange(rig.name);
                  }}
                >
                  <strong>{$rigLabel(rig.name, rig).title}</strong>
                  <span>{$rigLabel(rig.name, rig).description}</span>
                </button>
              {/each}
            </div>
          {/if}
        </div>
        <span class="status-text">
          · {status.providerName || status.provider} {status.model}
        </span>
      {:else}
        <span class="status-text">
          {rigTitle} · {status.providerName || status.provider} {status.model}
        </span>
      {/if}
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
