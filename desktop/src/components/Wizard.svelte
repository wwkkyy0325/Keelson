<script lang="ts">
  import { onMount, untrack } from "svelte";
  import {
    completeOnboarding,
    modelCatalog,
    saveKey,
    saveLocale,
    saveModel,
  } from "../lib/api";
  import {
    defaultSelection,
    findSelection,
    formatContext,
    type ModelRow,
  } from "../lib/catalog";
  import { locale, t } from "../lib/i18n";
  import type { ModelCatalog, SetupState } from "../lib/protocol";
  import ModelPicker from "./ModelPicker.svelte";

  let {
    initial,
    initialError = null,
    onDone,
  }: {
    initial: SetupState;
    initialError?: string | null;
    onDone: () => Promise<string | null>;
  } = $props();

  let step = $state(0);
  let keyDraft = $state("");
  let revealKey = $state(false);
  let hasKey = $state(untrack(() => initial.hasKey));
  let keyProvider = $state(untrack(() => initial.keyProvider));
  let manual = $state(false);
  let offline = $state(false);
  let providerKind = $state(
    untrack(() =>
      initial.provider === "anthropic-compatible"
        ? "anthropic-compatible"
        : "openai-compatible",
    ),
  );
  let baseUrl = $state(untrack(() => initial.manual?.baseUrl ?? ""));
  let modelName = $state(untrack(() => initial.manual?.model ?? ""));
  let error = $state<string | null>(untrack(() => initialError));
  let busy = $state(false);

  let catalog = $state<ModelCatalog | null>(null);
  let catalogLoading = $state(true);
  let catalogRefreshing = $state(false);
  let catalogError = $state<string | null>(null);
  let selectedProviderId = $state("");
  let selectedModelId = $state("");

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
      ? manualMatchesSelection
        ? selectedProviderId
        : "custom"
      : selectedProviderId,
  );
  let activeProviderName = $derived(
    manual
      ? manualMatchesSelection
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
  let activeModelLabel = $derived(
    manual
      ? manualMatchesSelection
        ? (selectedModel?.name ?? modelName)
        : modelName
      : (selectedModel?.name ?? selectedModelId),
  );
  let connectionChanged = $derived(
    initial.provider !== activeProviderKind ||
      initial.baseUrl !== activeBaseUrl ||
      initial.model !== activeModel ||
      (initial.providerId !== "" && initial.providerId !== activeProviderId),
  );
  let hasKeyForSelection = $derived(
    hasKey &&
      (keyProvider
        ? keyProvider === activeProviderId
        : !connectionChanged),
  );

  onMount(() => {
    void loadCatalog(false);
  });

  async function loadCatalog(refresh: boolean): Promise<void> {
    if (refresh) catalogRefreshing = true;
    else catalogLoading = true;
    catalogError = null;
    try {
      const value = await modelCatalog(refresh);
      catalog = value;
      const current = findSelection(value, selectedProviderId, selectedModelId);
      if (current) return;
      const choice = defaultSelection(value, initial, !initial.onboarded);
      if (choice) {
        choose(choice);
        manual = false;
      } else if (!manual) {
        manual = true;
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
    offline = false;
    error = null;
  }

  function toggleManual(): void {
    if (!manual) {
      const remembered = initial.manual;
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
    } else {
      manual = false;
      if (!selectedModel && catalog) {
        const choice = defaultSelection(catalog, initial, true);
        if (choice) choose(choice);
      }
    }
    offline = false;
    error = null;
  }

  function kindLabel(kind: string): string {
    return kind === "anthropic-compatible"
      ? $t("wizard.model.anthropic")
      : $t("wizard.model.openai");
  }

  async function nextFromConnect(): Promise<void> {
    offline = false;
    if (!manual && !selectedModel) {
      error = $t("wizard.error.select");
      return;
    }
    if (manual && (!baseUrl.trim() || !modelName.trim())) {
      error = $t("wizard.error.model");
      return;
    }
    if (!keyDraft.trim() && !hasKeyForSelection) {
      error = $t("wizard.error.key");
      return;
    }
    busy = true;
    error = null;
    try {
      if (keyDraft.trim()) {
        await saveKey(keyDraft.trim(), activeProviderId);
        keyDraft = "";
        hasKey = true;
        keyProvider = activeProviderId;
      }
      await saveModel(
        activeProviderKind,
        activeProviderId,
        activeProviderName,
        activeBaseUrl,
        activeModel,
        manual ? null : (selectedModel?.context ?? null),
      );
      go(1);
    } catch (cause) {
      error = $t("wizard.error.save", { message: errText(cause) });
    } finally {
      busy = false;
    }
  }

  async function useOffline(): Promise<void> {
    busy = true;
    error = null;
    try {
      offline = true;
      manual = false;
      await saveModel("mock", "mock", $t("wizard.model.mock"), "", "", null);
      go(1);
    } catch (cause) {
      error = $t("wizard.error.save", { message: errText(cause) });
    } finally {
      busy = false;
    }
  }

  function go(next: number): void {
    step = next;
    error = null;
  }

  async function changeLocale(next: "zh" | "en"): Promise<void> {
    locale.set(next);
    try {
      await saveLocale(next);
    } catch (cause) {
      error = $t("wizard.error.save", { message: errText(cause) });
    }
  }

  async function finish(): Promise<void> {
    busy = true;
    error = null;
    try {
      await completeOnboarding();
      const failure = await onDone();
      if (failure) error = failure;
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

<div class="wizard">
  <div class="wizard-card">
    <div class="wizard-head">
      <div class="brand">Keelson</div>
      <div class="progress-label">
        {$t("wizard.step", { current: step + 1, total: 3 })}
      </div>
    </div>
    <div class="progress-track">
      <div
        class="progress-fill"
        style={`width:${((step + 1) / 3) * 100}%`}
      ></div>
    </div>

    <!-- All steps share one grid cell, so the card height never jumps. -->
    <div class="stage">
      <section
        class="step wizard-step"
        class:active={step === 0}
        inert={step !== 0}
        aria-hidden={step !== 0}
      >
        <div class="step-body">
        <h1>{$t("wizard.connect.title")}</h1>
        <p class="muted">{$t("wizard.connect.subtitle")}</p>

        {#if catalog}
          <div class="segmented" role="group" aria-label={$t("wizard.connect.title")}>
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
        {/if}

        {#if manual}
          <div class="manual-fields fill">
            <label>
              <span>{$t("wizard.model.protocol")}</span>
              <select bind:value={providerKind}>
                <option value="anthropic-compatible">
                  {$t("wizard.model.anthropic")}
                </option>
                <option value="openai-compatible">
                  {$t("wizard.model.openai")}
                </option>
              </select>
            </label>
            <label>
              <span>{$t("wizard.model.baseUrl")}</span>
              <input
                bind:value={baseUrl}
                placeholder={$t("wizard.model.baseUrlPlaceholder")}
                spellcheck="false"
              />
            </label>
            <label>
              <span>{$t("wizard.model.model")}</span>
              <input
                bind:value={modelName}
                placeholder={$t("wizard.model.modelPlaceholder")}
                spellcheck="false"
              />
            </label>
            <p class="hint">{$t("wizard.manual.hint")}</p>
          </div>
        {:else}
          <ModelPicker
            fill
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

        <div class="connect-key">
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
          <p class="hint">{$t("wizard.key.subtitle")}</p>
        </div>

        {#if initial.problems.length > 0}
          <details class="problems">
            <summary>
              {$t("wizard.problems", { count: initial.problems.length })}
            </summary>
            <pre>{initial.problems.join("\n")}</pre>
          </details>
        {/if}
        </div>
        {#if error}<div class="banner">{error}</div>{/if}
        <div class="actions">
          <div class="action-links">
            <button class="link" onclick={useOffline} disabled={busy}>
              {$t("wizard.connect.offline")}
            </button>
          </div>
          <button
            class="primary"
            onclick={nextFromConnect}
            disabled={busy || (!manual && !selectedModel)}
          >
            {$t("wizard.connect.next")}
          </button>
        </div>
      </section>

      <section
        class="step wizard-step centered"
        class:active={step === 1}
        inert={step !== 1}
        aria-hidden={step !== 1}
      >
        <div class="step-body">
        <h1>{$t("wizard.language.title")}</h1>
        <p class="muted">{$t("wizard.language.subtitle")}</p>
        <div class="choices">
          <button class:active={$locale === "zh"} onclick={() => changeLocale("zh")}>
            <strong>中文</strong>
            <span>中文界面</span>
          </button>
          <button class:active={$locale === "en"} onclick={() => changeLocale("en")}>
            <strong>English</strong>
            <span>English interface</span>
          </button>
        </div>
        </div>
        {#if error}<div class="banner">{error}</div>{/if}
        <div class="actions">
          <button class="link" onclick={() => go(0)} disabled={busy}>
            {$t("wizard.back")}
          </button>
          <button class="primary" onclick={() => go(2)} disabled={busy}>
            {$t("wizard.language.next")}
          </button>
        </div>
      </section>

      <section
        class="step wizard-step centered"
        class:active={step === 2}
        inert={step !== 2}
        aria-hidden={step !== 2}
      >
        <div class="step-body">
        <div class="finish-mark">✓</div>
        <h1>{$t("wizard.finish.title")}</h1>
        <p class="muted">{$t("wizard.finish.subtitle")}</p>
        <div class="summary">
          <div>
            <span>{$t("wizard.finish.key")}</span>
            <strong>
              {offline
                ? $t("wizard.finish.offline")
                : hasKey
                  ? $t("wizard.finish.keySaved")
                  : $t("wizard.finish.key")}
            </strong>
          </div>
          <div>
            <span>{$t("wizard.finish.model")}</span>
            <strong>
              {offline
                ? $t("wizard.model.mock")
                : `${activeProviderName} · ${activeModelLabel}`}
            </strong>
          </div>
          <div>
            <span>{$t("wizard.finish.context")}</span>
            <strong>
              {offline ? "—" : formatContext(selectedModel?.context ?? null)}
            </strong>
          </div>
          <div>
            <span>{$t("wizard.finish.language")}</span>
            <strong>{$locale === "zh" ? "中文" : "English"}</strong>
          </div>
        </div>
        </div>
        {#if error}<div class="banner">{error}</div>{/if}
        <div class="actions">
          <button class="link" onclick={() => go(1)} disabled={busy}>
            {$t("wizard.back")}
          </button>
          <button class="primary" onclick={finish} disabled={busy}>
            {busy ? $t("wizard.finish.busy") : $t("wizard.finish.start")}
          </button>
        </div>
      </section>
    </div>
  </div>
</div>
