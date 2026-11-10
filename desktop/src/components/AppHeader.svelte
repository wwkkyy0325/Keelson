<script lang="ts">
  import { t } from "../lib/i18n";

  let {
    title,
    archived,
    onSettings,
  }: {
    title: string;
    archived: boolean;
    onSettings: () => void;
  } = $props();
</script>

<header>
  <div class="title" title={title}>{title}</div>
  {#if archived}
    <span class="archived-badge">{$t("app.archived")}</span>
  {/if}
  <div class="spacer"></div>
  <button class="icon" onclick={onSettings} title={$t("app.settings")}>
    ⚙
  </button>
</header>
