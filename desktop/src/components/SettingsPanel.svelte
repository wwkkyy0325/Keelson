<script lang="ts">
  import { onMount } from "svelte";
  import {
    modelCatalog,
    saveKey,
    saveLocale,
    saveModel,
    setupState,
  } from "../lib/api";
  import { defaultSelection, findSelection, type ModelRow } from "../lib/catalog";
  import { locale, t } from "../lib/i18n";
  import type { ModelCatalog, SetupState } from "../lib/protocol";
  import ModelPicker from "./ModelPicker.svelte";

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
  let catalog = $state<ModelCatalog | null>(null);
  let catalogLoading = $state(true);
  let catalogRefreshing = $state(false);
  let catalogError = $state<string | null>(null);
  let selectedProviderId = $state("");
  let selectedModelId = $state("");
  let manual = $state(false);
  let providerKind = $state("openai-compatible");
  let baseUrl = $state("");
  let modelName = $state("");
  let keyDraft = $state("");
  let revealKey = $state(false);
  let busy = $state(false);
  let loading = $state(true);
  let error = $state<string | null>(null);

  let selectedProvider = $derived(
    catalog?.providers.find((provider) => provider.id === selectedProviderId) ??
      null,
  );
  let selectedModel = $derived(
    selectedProvider?.models.find((model) => model.id === selectedModelId) ?? null,
  );
  let manualMatchesSelection = $derived(
    !!selectedProvider &&
      providerKind === selectedProvider.kind &&
      baseUrl === selectedProvider.baseUrl &&
      modelName === selectedModelId,
  );
  let activeProviderId = $derived(
    manual
      ? providerKind === "mock"
        ? "mock"
        : manualMatchesSelection
          ? selectedProviderId
          : "custom"
      : selectedProviderId,
  );
  let activeProviderName = $derived(
    manual
      ? providerKind === "mock"
        ? $t("wizard.model.mock")
        : manualMatchesSelection
          ? (selectedProvider?.name ?? "")
          : kindLabel(providerKind)
      : (selectedProvider?.name ?? ""),
  );
  let activeProviderKind = $derived(
    manual ? providerKind : (selectedProvider?.kind ?? ""),
  );
  let activeBaseUrl = $derived(
    manual ? baseUrl : (selectedProvider?.baseUrl ?? ""),
  );
  let activeModel = $derived(manual ? modelName : selectedModelId);
  let isMock = $derived(activeProviderKind === "mock");
  let hasKeyForSelection = $derived(
    !!setup?.keyProviders?.includes(activeProviderId),
  );

  onMount(async () => {
    try {
      const state = await setupState();
      setup = state;
      providerKind =
        state.provider === "anthropic-compatible"
          ? "anthropic-compatible"
          : state.provider === "mock"
            ? "mock"
            : "openai-compatible";
      baseUrl = state.baseUrl;
      modelName = state.model;
      await loadCatalog(false, state);
    } catch (cause) {
      error = errText(cause);
      catalogLoading = false;
    } finally {
      loading = false;
    }
  });

  async function loadCatalog(
    refresh: boolean,
    state = setup,
  ): Promise<void> {
    if (refresh) catalogRefreshing = true;
    else catalogLoading = true;
    catalogError = null;
    try {
      const value = await modelCatalog(refresh);
      catalog = value;
      if (state) {
        const current = findSelection(value, selectedProviderId, selectedModelId);
        if (current) return;
        if (state.provider === "mock") {
          manual = true;
          return;
        }
        const choice = defaultSelection(value, state, !state.model);
        if (choice) {
          choose(choice);
        } else if (!manual) {
          manual = true;
        }
      }
    } catch (cause) {
      catalogError = errText(cause);
      if (!catalog) manual = true;
    } finally {
      catalogLoading = false;
      catalogRefreshing = false;
    }
  }

  function choose(row: ModelRow): void {
    selectedProviderId = row.provider.id;
    selectedModelId = row.model.id;
    manual = false;
    error = null;
  }

  function toggleManual(): void {
    if (manual) {
      manual = false;
      if (!selectedProvider && catalog && setup) {
        const choice = defaultSelection(catalog, setup, true);
        if (choice) choose(choice);
      }
    } else {
      const remembered = setup?.manual;
      if (remembered) {
        providerKind = remembered.provider;
        baseUrl = remembered.baseUrl;
        modelName = remembered.model;
      } else {
        // Nothing has been saved by hand yet: an empty form with examples is
        // clearer than another provider's address pre-filled.
        providerKind = selectedProvider?.kind ?? providerKind;
        baseUrl = "";
        modelName = "";
      }
      manual = true;
    }
    error = null;
  }

  function kindLabel(kind: string): string {
    return kind === "anthropic-compatible"
      ? $t("wizard.model.anthropic")
      : $t("wizard.model.openai");
  }

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
    if (!manual && !selectedProvider) {
      error = $t("wizard.error.select");
      return;
    }
    if (!isMock && (!activeBaseUrl.trim() || !activeModel.trim())) {
      error = $t("wizard.error.model");
      return;
    }
    if (!isMock && !hasKeyForSelection && !keyDraft.trim()) {
      error = setup?.hasKey
        ? $t("settings.keyOther")
        : $t("settings.error.key");
      return;
    }
    busy = true;
    try {
      if (keyDraft.trim()) {
        await saveKey(keyDraft.trim(), activeProviderId);
        if (setup) {
          setup.hasKey = true;
          setup.keyProvider = activeProviderId;
        }
        keyDraft = "";
      }
      await saveModel(
        activeProviderKind,
        activeProviderId,
        activeProviderName,
        activeBaseUrl,
        activeModel,
        manual ? null : (selectedModel?.context ?? null),
      );
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

        <div class="segmented" role="group" aria-label={$t("settings.model")}>
          <button
            type="button"
            class:active={!manual}
            onclick={() => {
              if (manual) toggleManual();
            }}
            disabled={busy}
          >
            {$t("model.modeList")}
          </button>
          <button
            type="button"
            class:active={manual}
            onclick={() => {
              if (!manual) toggleManual();
            }}
            disabled={busy}
          >
            {$t("model.modeManual")}
          </button>
        </div>

        {#if manual}
          <div class="manual-fields">
            <label>
              <span>{$t("wizard.model.protocol")}</span>
              <select bind:value={providerKind}>
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
                disabled={providerKind === "mock"}
                placeholder={$t("wizard.model.baseUrlPlaceholder")}
                spellcheck="false"
              />
            </label>
            <label>
              <span>{$t("wizard.model.model")}</span>
              <input
                bind:value={modelName}
                disabled={providerKind === "mock"}
                placeholder={$t("wizard.model.modelPlaceholder")}
                spellcheck="false"
              />
            </label>
          </div>
        {:else}
          <ModelPicker
            catalog={catalog}
            loading={catalogLoading}
            refreshing={catalogRefreshing}
            error={catalogError}
            selectedProviderId={selectedProviderId}
            selectedModelId={selectedModelId}
            onSelect={choose}
            onRefresh={() => loadCatalog(true)}
          />
        {/if}

        {#if !isMock}
          <label>
            <span>{$t("settings.key")}</span>
            <div class="key-row">
              <input
                type={revealKey ? "text" : "password"}
                bind:value={keyDraft}
                placeholder={hasKeyForSelection
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
          {#if hasKeyForSelection}
            <p class="hint">{$t("settings.keySavedHint")}</p>
          {/if}
          {#if manual}
            <p class="hint">{$t("settings.keyLocalHint")}</p>
          {/if}
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
      <button
        class="primary"
        onclick={save}
        disabled={busy || loading || (!manual && !selectedProvider)}
      >
        {busy ? $t("settings.saving") : $t("settings.save")}
      </button>
    </div>
  </div>
</div>
