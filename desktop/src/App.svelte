<script lang="ts">
  import { onMount } from "svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { listenToEvents, openSession, setupState } from "./lib/api";
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
    locale: "zh",
    lastWorkspace: "",
    keyProvider: "",
    keyProviders: [],
    manual: null,
    rig: "",
    rigs: [],
    problems: [],
  };

  let phase = $state<"loading" | "wizard" | "app">("loading");
  let setup = $state<SetupState | null>(null);
  let bootError = $state<string | null>(null);
  let showSettings = $state(false);
  let unlisteners: UnlistenFn[] = [];

  onMount(() => {
    let disposed = false;
    (async () => {
      try {
        unlisteners = await listenToEvents();
        const state = await setupState();
        if (disposed) return;
        setup = state;
        locale.set(state.locale === "en" ? "en" : "zh");
        if (state.onboarded && state.hasKey) {
          const failure = await enterApp();
          if (failure) {
            bootError = failure;
            phase = "wizard";
          }
        } else {
          phase = "wizard";
        }
      } catch (error) {
        if (disposed) return;
        bootError = errText(error);
        phase = "wizard";
      }
    })();
    return () => {
      disposed = true;
      for (const unlisten of unlisteners) unlisten();
    };
  });

  async function enterApp(
    workspace?: string,
    sessionId?: string,
  ): Promise<string | null> {
    try {
      const view = await openSession(workspace ?? null, sessionId ?? null);
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

  async function reconnect(): Promise<string | null> {
    return enterApp(
      $session.workspace || undefined,
      $session.sessionId || undefined,
    );
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
  </div>
{:else if phase === "wizard"}
  <Wizard
    initial={setup ?? fallbackSetup}
    initialError={bootError}
    onDone={onWizardDone}
  />
{:else}
  <Chat onSettings={() => (showSettings = true)} />
  {#if showSettings}
    <SettingsPanel
      workspace={$session.workspace}
      onClose={() => (showSettings = false)}
      onReconnect={reconnect}
    />
  {/if}
{/if}
