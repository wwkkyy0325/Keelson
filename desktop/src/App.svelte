<script lang="ts">
  import { onMount } from "svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import {
    cancelTurn,
    forceRecover,
    listenToEvents,
    openSession,
    setupState,
  } from "./lib/api";
  import { dispatch, session } from "./lib/session-store";
  import { locale, t } from "./lib/i18n";
  import type { SetupState } from "./lib/protocol";
  import Chat from "./components/Chat.svelte";
  import SettingsPanel from "./components/SettingsPanel.svelte";
  import Wizard from "./components/Wizard.svelte";

  const fallbackSetup: SetupState = {
    onboarded: false,
    hasKey: false,
    provider: "openai-compatible",
    providerId: "",
    providerName: "",
    baseUrl: "https://api.deepseek.com",
    model: "deepseek-flash",
    modelContext: null,
    locale: "zh",
    lastWorkspace: "",
    keyProvider: "",
    keyProviders: [],
    manual: null,
    keySlot: "",
    presets: [],
    activePreset: null,
    journalMode: "project",
    journalPath: "",
    rig: "",
    rigs: [],
    problems: [],
  };

  let phase = $state<"loading" | "wizard" | "app" | "failed">("loading");
  let setup = $state<SetupState | null>(null);
  let bootError = $state<string | null>(null);
  let showSettings = $state(false);
  let unlisteners: UnlistenFn[] = [];
  /// Seconds since the window started waiting. Shown after a few seconds so a
  /// slow boot reads as slow instead of dead, next to the way out of it.
  let bootElapsed = $state(0);

  /// A backend that never answers must not leave the boot screen up forever.
  function withTimeout<T>(
    promise: Promise<T>,
    ms: number,
    message: string,
  ): Promise<T> {
    return Promise.race([
      promise,
      new Promise<T>((_, reject) =>
        setTimeout(() => reject(new Error(message)), ms),
      ),
    ]);
  }

  onMount(() => {
    let disposed = false;
    const started = Date.now();
    const clock = setInterval(() => {
      bootElapsed = Math.floor((Date.now() - started) / 1000);
    }, 500);
    (async () => {
      try {
        unlisteners = await withTimeout(
          listenToEvents(),
          10000,
          $t("app.bootTimeout"),
        );
        const state = await withTimeout(
          setupState(),
          10000,
          $t("app.bootTimeout"),
        );
        if (disposed) return;
        setup = state;
        locale.set(state.locale === "en" ? "en" : "zh");
        if (state.onboarded && state.hasKey) {
          const failure = await enterApp();
          if (failure) {
            // The setup wizard is for a first run. An onboarded app that
            // cannot open its session (a turn survived a webview reload,
            // say) gets a retry screen instead; the wizard would be a trap.
            bootError = failure;
            phase = "failed";
          }
        } else {
          phase = "wizard";
        }
      } catch (error) {
        if (disposed) return;
        bootError = errText(error);
        // Only a first run belongs in the wizard. If the setup read itself
        // failed we cannot tell, and the retry screen is the safe guess: the
        // wizard has no way back and no recovery.
        phase = setup?.onboarded === false ? "wizard" : "failed";
      }
    })();
    return () => {
      disposed = true;
      clearInterval(clock);
      for (const unlisten of unlisteners) unlisten();
    };
  });

  async function enterApp(
    workspace?: string,
    sessionId?: string,
  ): Promise<string | null> {
    try {
      const view = await withTimeout(
        openSession(workspace ?? null, sessionId ?? null),
        15000,
        $t("app.openTimeout"),
      );
      dispatch({ type: "session-opened", view });
      phase = "app";
      return null;
    } catch (error) {
      return errText(error);
    }
  }

  async function onWizardDone(): Promise<string | null> {
    return enterApp();
  }

  /// A reload while a turn runs leaves the turn in the engine and the new
  /// frontend without its cancel handle. Cancel it, then wait for the turn to
  /// unwind before opening: the engine refuses a new session until then.
  async function cancelAndRetry(): Promise<void> {
    phase = "loading";
    bootError = null;
    await cancelTurn(null).catch(() => false);
    let failure = "";
    for (let attempt = 0; attempt < 6; attempt += 1) {
      failure = (await enterApp()) ?? "";
      if (!failure || !failure.includes("还在运行")) break;
      await new Promise((resolve) => setTimeout(resolve, 300));
    }
    // A turn that ignores the cancel must not hold the workspace hostage: the
    // engine is dropped and rebuilt, and the abandoned turn still writes its
    // own result into the document.
    if (failure && failure.includes("还在运行")) {
      await forceRecover().catch(() => false);
      failure = (await enterApp()) ?? "";
    }
    if (failure) {
      bootError = failure;
      phase = "failed";
    }
  }

  /// The last resort: the backend forgets every open conversation (cancelling
  /// whatever streams into them) and the window opens again with a fresh
  /// engine. It also works after a reload that stranded a turn.
  async function recoverNow(): Promise<void> {
    phase = "loading";
    bootError = null;
    await forceRecover().catch(() => false);
    const failure = await enterApp();
    if (failure) {
      bootError = failure;
      phase = "failed";
    }
  }

  async function retryBoot(): Promise<void> {
    phase = "loading";
    bootError = null;
    const failure = await enterApp();
    if (failure) {
      bootError = failure;
      phase = "failed";
    }
  }

  async function reconnect(): Promise<string | null> {
    return enterApp(
      $session.workspace || undefined,
      $session.sessionId || undefined,
    );
  }

  /// The status bar's preset list is read from the config after a save,
  /// apply or delete; a failed refresh keeps the last list instead of
  /// replacing it with an error.
  async function refreshSetup(): Promise<void> {
    try {
      setup = await setupState();
    } catch {
      // The next boot retries.
    }
  }

  function errText(error: unknown): string {
    if (typeof error === "string") return error;
    if (error instanceof Error) return error.message;
    return String(error);
  }
</script>

{#if phase === "loading"}
  <div class="boot">
    <div class="brand">Keelson</div>
    <p class="muted">{$t("app.loadingApp")}</p>
    {#if bootElapsed >= 3}
      <p class="muted">
        {$t("app.loadingElapsed", { seconds: bootElapsed })}
      </p>
      <div class="boot-actions">
        <button
          onclick={() => {
            bootError = $t("app.bootTimeout");
            phase = "failed";
          }}>{$t("app.skipWait")}</button
        >
        <button class="primary" onclick={recoverNow}>
          {$t("app.recover")}
        </button>
      </div>
    {/if}
  </div>
{:else if phase === "wizard"}
  <Wizard
    initial={setup ?? fallbackSetup}
    initialError={bootError}
    onDone={onWizardDone}
  />
{:else if phase === "failed"}
  <div class="boot">
    <div class="brand">Keelson</div>
    <p class="muted">{$t("app.bootFailed")}</p>
    <p class="boot-error" role="alert">{bootError}</p>
    <div class="boot-actions">
      <button class="primary" onclick={cancelAndRetry}>
        {$t("app.cancelTurnRetry")}
      </button>
      <button onclick={retryBoot}>{$t("app.retry")}</button>
      <button onclick={recoverNow}>{$t("app.recover")}</button>
    </div>
    <p class="hint">{$t("app.recoverHint")}</p>
  </div>
{:else}
  <Chat
    onSettings={() => (showSettings = true)}
    rigs={setup?.rigs ?? []}
    presets={setup?.presets ?? []}
    activePreset={setup?.activePreset ?? null}
    onPresetsChanged={refreshSetup}
  />
  {#if showSettings}
    <SettingsPanel
      workspace={$session.workspace}
      onClose={() => {
        showSettings = false;
        // Saving a connection detaches it from its preset; the status bar's
        // mark must follow.
        void refreshSetup();
      }}
      onReconnect={reconnect}
    />
  {/if}
{/if}
