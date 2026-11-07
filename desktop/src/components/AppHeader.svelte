<script lang="ts">
  import { saveLocale } from "../lib/api";
  import { locale, t } from "../lib/i18n";

  let {
    title,
    archived,
    onSettings,
  }: {
    title: string;
    archived: boolean;
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
</script>

<header>
  <div class="title" title={title}>{title}</div>
  {#if archived}
    <span class="archived-badge">{$t("app.archived")}</span>
  {/if}
  <div class="spacer"></div>
  <button class="icon" onclick={toggleLocale} title={$t("app.switchLanguage")}>
    {$locale === "zh" ? "EN" : "中"}
  </button>
  <button class="icon" onclick={onSettings} title={$t("app.settings")}>
    ⚙
  </button>
</header>
