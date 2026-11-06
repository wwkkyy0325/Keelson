<script lang="ts">
  import { saveLocale } from "../lib/api";
  import { locale, t } from "../lib/i18n";

  let {
    workspace,
    onWorkspace,
    onSettings,
  }: {
    workspace: string;
    onWorkspace: () => void;
    onSettings: () => void;
  } = $props();

  async function toggleLocale(): Promise<void> {
    const next = $locale === "zh" ? "en" : "zh";
    locale.set(next);
    try {
      await saveLocale(next);
    } catch {
      // The interface already switched; the next launch can retry.
    }
  }

  function baseName(path: string): string {
    const parts = path.split(/[\\/]/).filter(Boolean);
    return parts.length > 0 ? parts[parts.length - 1] : path;
  }
</script>

<header>
  <div class="title">Keelson</div>
  <button class="workspace" onclick={onWorkspace} title={workspace}>
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    </svg>
    <span>{baseName(workspace)}</span>
  </button>
  <div class="spacer"></div>
  <button class="icon" onclick={toggleLocale} title={$t("app.switchLanguage")}>
    {$locale === "zh" ? "EN" : "中"}
  </button>
  <button class="icon" onclick={onSettings} title={$t("app.settings")}>
    ⚙
  </button>
</header>
