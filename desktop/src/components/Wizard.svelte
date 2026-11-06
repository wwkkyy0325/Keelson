<script lang="ts">
  import { untrack } from "svelte";
  import {
    completeOnboarding,
    saveKey,
    saveLocale,
    saveModel,
  } from "../lib/api";
  import { locale, t } from "../lib/i18n";
  import type { SetupState } from "../lib/protocol";

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
  // The wizard is mounted once; capture the initial setup deliberately.
  let provider = $state(untrack(() => initial.provider));
  let baseUrl = $state(untrack(() => initial.baseUrl));
  let modelName = $state(untrack(() => initial.model));
  let hasKey = $state(untrack(() => initial.hasKey));
  let error = $state<string | null>(untrack(() => initialError));
  let busy = $state(false);

  function go(next: number): void {
    step = next;
    error = null;
  }

  async function nextFromKey(): Promise<void> {
    if (!keyDraft.trim() && !hasKey) {
      error = $t("wizard.error.key");
      return;
    }
    busy = true;
    try {
      if (keyDraft.trim()) {
        await saveKey(keyDraft.trim());
        hasKey = true;
        keyDraft = "";
      }
      go(1);
    } catch (cause) {
      error = $t("wizard.error.save", { message: errText(cause) });
    } finally {
      busy = false;
    }
  }

  async function nextFromModel(): Promise<void> {
    if (provider !== "mock" && (!baseUrl.trim() || !modelName.trim())) {
      error = $t("wizard.error.model");
      return;
    }
    busy = true;
    try {
      await saveModel(provider, baseUrl, modelName);
      go(2);
    } catch (cause) {
      error = $t("wizard.error.save", { message: errText(cause) });
    } finally {
      busy = false;
    }
  }

  async function changeLocale(next: "zh" | "en"): Promise<void> {
    locale.set(next);
    try {
      await saveLocale(next);
    } catch (cause) {
      error = $t("wizard.error.save", { message: errText(cause) });
    }
  }

  async function useOffline(): Promise<void> {
    busy = true;
    try {
      provider = "mock";
      await saveModel("mock", baseUrl, modelName);
      go(2);
    } catch (cause) {
      error = $t("wizard.error.save", { message: errText(cause) });
    } finally {
      busy = false;
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
        {$t("wizard.step", { current: step + 1, total: 4 })}
      </div>
    </div>
    <div class="progress-track">
      <div
        class="progress-fill"
        style={`width:${((step + 1) / 4) * 100}%`}
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
        <h1>{$t("wizard.key.title")}</h1>
        <p class="muted">{$t("wizard.key.subtitle")}</p>
        <div class="key-row">
          <input
            type={revealKey ? "text" : "password"}
            bind:value={keyDraft}
            placeholder={hasKey
              ? $t("settings.keySaved")
              : $t("wizard.key.placeholder")}
            autocomplete="off"
            spellcheck="false"
          />
          <button class="ghost" onclick={() => (revealKey = !revealKey)}>
            {revealKey ? $t("wizard.key.hide") : $t("wizard.key.show")}
          </button>
        </div>
        <p class="hint">{$t("wizard.key.offlineHint")}</p>
        {#if error}<div class="banner">{error}</div>{/if}
        <div class="actions">
          <button class="link" onclick={useOffline} disabled={busy}>
            {$t("wizard.key.offline")}
          </button>
          <button class="primary" onclick={nextFromKey} disabled={busy}>
            {$t("wizard.key.next")}
          </button>
        </div>
      </section>

      <section
        class="step wizard-step"
        class:active={step === 1}
        inert={step !== 1}
        aria-hidden={step !== 1}
      >
        <h1>{$t("wizard.model.title")}</h1>
        <p class="muted">{$t("wizard.model.subtitle")}</p>
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
        {#if initial.problems.length > 0}
          <details class="problems">
            <summary>
              {$t("wizard.problems", { count: initial.problems.length })}
            </summary>
            <pre>{initial.problems.join("\n")}</pre>
          </details>
        {/if}
        {#if error}<div class="banner">{error}</div>{/if}
        <div class="actions">
          <button class="link" onclick={() => go(0)} disabled={busy}>
            {$t("wizard.back")}
          </button>
          <button class="primary" onclick={nextFromModel} disabled={busy}>
            {$t("wizard.model.next")}
          </button>
        </div>
      </section>

      <section
        class="step wizard-step"
        class:active={step === 2}
        inert={step !== 2}
        aria-hidden={step !== 2}
      >
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
        {#if error}<div class="banner">{error}</div>{/if}
        <div class="actions">
          <button class="link" onclick={() => go(1)} disabled={busy}>
            {$t("wizard.back")}
          </button>
          <button class="primary" onclick={() => go(3)} disabled={busy}>
            {$t("wizard.language.next")}
          </button>
        </div>
      </section>

      <section
        class="step wizard-step"
        class:active={step === 3}
        inert={step !== 3}
        aria-hidden={step !== 3}
      >
        <div class="finish-mark">✓</div>
        <h1>{$t("wizard.finish.title")}</h1>
        <p class="muted">{$t("wizard.finish.subtitle")}</p>
        <div class="summary">
          <div>
            <span>{$t("wizard.finish.key")}</span>
            <strong>
              {provider === "mock"
                ? $t("wizard.finish.offline")
                : $t("wizard.finish.keySaved")}
            </strong>
          </div>
          <div>
            <span>{$t("wizard.finish.model")}</span>
            <strong>
              {provider === "mock"
                ? $t("wizard.model.mock")
                : `${provider} · ${modelName}`}
            </strong>
          </div>
          <div>
            <span>{$t("wizard.finish.language")}</span>
            <strong>{$locale === "zh" ? "中文" : "English"}</strong>
          </div>
        </div>
        {#if error}<div class="banner">{error}</div>{/if}
        <div class="actions">
          <button class="link" onclick={() => go(2)} disabled={busy}>
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
