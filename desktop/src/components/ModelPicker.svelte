<script lang="ts">
  import { untrack } from "svelte";
  import { formatContext, formatPrice, modelsFor, type ModelRow } from "../lib/catalog";
  import { t } from "../lib/i18n";
  import type { ModelCatalog } from "../lib/protocol";

  let {
    catalog = null,
    loading = false,
    refreshing = false,
    error = null,
    selectedProviderId = "",
    selectedModelId = "",
    fill = false,
    onSelect,
    onRefresh,
  }: {
    catalog?: ModelCatalog | null;
    loading?: boolean;
    refreshing?: boolean;
    error?: string | null;
    selectedProviderId?: string;
    selectedModelId?: string;
    fill?: boolean;
    onSelect: (row: ModelRow) => void;
    onRefresh: () => void;
  } = $props();

  let query = $state("");
  let providerFilter = $state(untrack(() => selectedProviderId || "all"));
  let filterTouched = $state(false);
  let rows = $derived(catalog ? modelsFor(catalog, providerFilter, query) : []);

  // The parent selects a saved or default model after the catalog arrives.
  // Follow it once, then leave the filter alone after any user action.
  $effect(() => {
    if (!filterTouched && selectedProviderId) {
      providerFilter = selectedProviderId;
    }
  });

  function isSelected(row: ModelRow): boolean {
    return (
      row.provider.id === selectedProviderId && row.model.id === selectedModelId
    );
  }
</script>

<div class="catalog" class:fill>
  <div class="catalog-toolbar">
    <input
      class="catalog-search"
      bind:value={query}
      placeholder={$t("catalog.search")}
      aria-label={$t("catalog.search")}
      spellcheck="false"
    />
    <select
      class="catalog-provider"
      bind:value={providerFilter}
      onchange={() => (filterTouched = true)}
      aria-label={$t("catalog.provider")}
    >
      <option value="all">{$t("catalog.allProviders")}</option>
      {#each catalog?.providers ?? [] as provider (provider.id)}
        <option value={provider.id}>
          {provider.name} · {provider.models.length}
        </option>
      {/each}
    </select>
    <button
      type="button"
      class="catalog-refresh"
      onclick={onRefresh}
      disabled={refreshing || loading}
      title={refreshing ? $t("catalog.refreshing") : $t("catalog.refresh")}
      aria-label={refreshing ? $t("catalog.refreshing") : $t("catalog.refresh")}
    >
      <span aria-hidden="true">{refreshing ? "…" : "↻"}</span>
    </button>
  </div>

  <div class="catalog-list">
    {#if loading && !catalog}
      <p class="catalog-empty">{$t("catalog.loading")}</p>
    {:else if error && !catalog}
      <div class="catalog-error">
        <p>{$t("catalog.error", { message: error })}</p>
        <button class="link" onclick={onRefresh}>{$t("catalog.retry")}</button>
      </div>
    {:else}
      {#each rows as row (row.provider.id + "/" + row.model.id)}
        <button
          type="button"
          class="catalog-row"
          onclick={() => {
            filterTouched = true;
            onSelect(row);
          }}
          aria-pressed={isSelected(row)}
          title={row.model.id}
        >
          <span class="catalog-main">
            <span class="catalog-title-line">
              <span class="catalog-name">{row.model.name}</span>
              {#if providerFilter === "all"}
                <span class="catalog-by">{row.provider.name}</span>
              {/if}
            </span>
            <span class="catalog-id">{row.model.id}</span>
          </span>
          <span class="catalog-facts">
            {#if isSelected(row)}
              <span class="catalog-check" title={$t("catalog.selected")}>✓</span>
            {/if}
            {#if row.model.vision}
              <span class="catalog-badge vision" title={$t("catalog.visionHint")}>
                {$t("catalog.vision")}
              </span>
            {/if}
            {#if row.model.pdf}
              <span class="catalog-badge pdf" title={$t("catalog.pdfHint")}>
                PDF
              </span>
            {/if}
            {#if row.model.reasoning}
              <span class="catalog-badge">{$t("catalog.reasoning")}</span>
            {/if}
            <span class="catalog-context" title={$t("catalog.contextHint")}>
              {formatContext(row.model.context)}
            </span>
            <span class="catalog-price" title={$t("catalog.priceHint")}>
              {formatPrice(row.model.costInput)} / {formatPrice(
                row.model.costOutput,
              )}
            </span>
          </span>
        </button>
      {:else}
        <p class="catalog-empty">{$t("catalog.empty")}</p>
      {/each}
    {/if}
  </div>
</div>
