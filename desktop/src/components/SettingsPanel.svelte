<script lang="ts">
  import { onMount } from "svelte";
  import { saveKey, saveLocale, saveModel, setupState } from "../lib/api";
  import { locale, t } from "../lib/i18n";
  import type { SetupState } from "../lib/protocol";

  let {
    workspace,
    onClose,
    onReconnect,
  }: {
    workspace: string;
    onClose: () => void;
    onReconnect: () => Promise<string | null>;
  } = $props();

  let setup = $state<SetupState | null>(null);
  let provider = $state("anthropic-compatible");
  let baseUrl = $state("");
  let modelName = $state("");
  let keyDraft = $state("");
  let revealKey = $state(false);
  let busy = $state(false);
  let loading = $state(true);
  let error = $state<string | null>(null);

  onMount(async () => {
    try {
      const state = await setupState();
      setup = state;
      provider = state.provider;
      baseUrl = state.baseUrl;
      modelName = state.model;
    } catch (cause) {
      error = errText(cause);
    } finally {
      loading = false;
    }
  });

  async function changeLocale(next: "zh" | "en"): Promise<void> {
    locale.set(next);
    try {
      await saveLocale(next);
    } catch (cause) {
      error = $t("wizard.error.save", { message: errText(cause) });
    }
  }

  async function save(): Promise<void> {
    error = null;
    if (provider !== "mock" && !setup?.hasKey && !keyDraft.trim()) {
      error = $t("settings.error.key");
      return;
    }
    if (provider !== "mock" && (!baseUrl.trim() || !modelName.trim())) {
      error = $t("wizard.error.model");
      return;
    }
    busy = true;
    try {
      if (keyDraft.trim()) {
        await saveKey(keyDraft.trim());
        if (setup) setup.hasKey = true;
        keyDraft = "";
      }
      await saveModel(provider, baseUrl, modelName);
      const reconnectError = await onReconnect();
      if (reconnectError) {
        error = reconnectError;
        return;
      }
      onClose();
    } catch (cause) {
      error = $t("wizard.error.save", { message: errText(cause) });
    } finally {
      busy = false;
    }
  }

  function errText(cause: unknown): string {
    if (typeof cause === "string") return cause;
    if (cause instanceof Error) return cause.message;
    return String(cause);
  }
</script>

<div class="settings-layer">
  <button
    class="settings-backdrop"
    aria-label={$t("settings.close")}
    onclick={onClose}
  ></button>

  <div
    class="settings-panel"
    role="dialog"
    aria-modal="true"
    aria-labelledby="settings-title"
  >
    <div class="settings-head">
      <h2 id="settings-title">{$t("settings.title")}</h2>
      <button class="icon" onclick={onClose} title={$t("settings.close")}>
        ×
      </button>
    </div>

    <div class="settings-body">
      <section>
        <h3>{$t("settings.model")}</h3>
        <p class="muted">{$t("settings.modelHint")}</p>
        <label>
          <span>{$t("wizard.model.provider")}</span>
          <select bind:value={provider}>
            <option value="anthropic-compatible">
              {$t("wizard.model.anthropic")}
            </option>
            <option value="openai-compatible">
              {$t("wizard.model.openai")}
            </option>
            <option value="mock">{$t("wizard.model.mock")}</option>
          </select>
        </label>
        <label>
          <span>{$t("wizard.model.baseUrl")}</span>
          <input
            bind:value={baseUrl}
            disabled={provider === "mock"}
            spellcheck="false"
          />
        </label>
        <label>
          <span>{$t("wizard.model.model")}</span>
          <input
            bind:value={modelName}
            disabled={provider === "mock"}
            spellcheck="false"
          />
        </label>
        <label>
          <span>{$t("settings.key")}</span>
          <div class="key-row">
            <input
              type={revealKey ? "text" : "password"}
              bind:value={keyDraft}
              placeholder={setup?.hasKey
                ? $t("settings.keySaved")
                : $t("wizard.key.placeholder")}
              autocomplete="off"
              spellcheck="false"
            />
            <button class="ghost" onclick={() => (revealKey = !revealKey)}>
              {revealKey ? $t("wizard.key.hide") : $t("wizard.key.show")}
            </button>
          </div>
        </label>
        {#if setup?.hasKey}
          <p class="hint">{$t("settings.keySavedHint")}</p>
        {/if}
      </section>

      <section>
        <h3>{$t("settings.language")}</h3>
        <div class="segmented">
          <button class:active={$locale === "zh"} onclick={() => changeLocale("zh")}>
            中文
          </button>
          <button class:active={$locale === "en"} onclick={() => changeLocale("en")}>
            English
          </button>
        </div>
      </section>

      <section>
        <h3>{$t("settings.data")}</h3>
        <dl class="meta">
          <div>
            <dt>{$t("settings.workspace")}</dt>
            <dd title={workspace}>{workspace}</dd>
          </div>
        </dl>
        <p class="hint">{$t("settings.sessionHint")}</p>
      </section>

      {#if error}
        <div class="banner">{error}</div>
      {/if}
    </div>

    <div class="settings-foot">
      <button class="link" onclick={onClose}>{$t("settings.close")}</button>
      <button class="primary" onclick={save} disabled={busy || loading}>
        {busy ? $t("settings.saving") : $t("settings.save")}
      </button>
    </div>
  </div>
</div>
