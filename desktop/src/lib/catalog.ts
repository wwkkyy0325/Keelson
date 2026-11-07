import type {
  CatalogModel,
  CatalogProvider,
  ModelCatalog,
  SetupState,
} from "./protocol";

export type ModelRow = {
  provider: CatalogProvider;
  model: CatalogModel;
};

export function findSelection(
  catalog: ModelCatalog,
  providerId: string,
  modelId: string,
): ModelRow | null {
  const provider = catalog.providers.find((item) => item.id === providerId);
  const model = provider?.models.find((item) => item.id === modelId);
  return provider && model ? { provider, model } : null;
}

/// Prefers the saved model. With `allowDefault`, a first run falls back to the
/// first model of the default provider.
export function defaultSelection(
  catalog: ModelCatalog,
  setup: SetupState,
  allowDefault = true,
): ModelRow | null {
  const exact = findSelection(catalog, setup.providerId, setup.model);
  if (exact) return exact;
  for (const provider of catalog.providers) {
    const model = provider.models.find((item) => item.id === setup.model);
    if (model) return { provider, model };
  }
  if (!allowDefault) return null;
  const provider =
    catalog.providers.find((item) => item.id === "deepseek") ??
    catalog.providers[0];
  const model = provider?.models[0];
  return provider && model ? { provider, model } : null;
}

export function modelsFor(
  catalog: ModelCatalog,
  providerId: string,
  query: string,
): ModelRow[] {
  const needle = query.trim().toLowerCase();
  const rows: ModelRow[] = [];
  for (const provider of catalog.providers) {
    if (providerId !== "all" && provider.id !== providerId) continue;
    for (const model of provider.models) {
      if (
        needle &&
        !model.name.toLowerCase().includes(needle) &&
        !model.id.toLowerCase().includes(needle) &&
        !provider.name.toLowerCase().includes(needle)
      ) {
        continue;
      }
      rows.push({ provider, model });
    }
  }
  // The unscoped list is a browse view, not the whole catalog. The cap keeps
  // the DOM small; a provider-scoped list is small enough to show whole.
  const limit = providerId === "all" ? (needle ? 200 : 80) : rows.length;
  return rows.slice(0, limit);
}

export function formatContext(tokens: number | null): string {
  if (!tokens) return "—";
  if (tokens >= 1_000_000) {
    return `${Math.round((tokens / 1_000_000) * 10) / 10}M`;
  }
  if (tokens >= 1_000) return `${Math.round(tokens / 1_000)}K`;
  return String(tokens);
}

export function formatPrice(cost: number | null): string {
  if (cost === null) return "—";
  if (cost === 0) return "$0";
  if (cost >= 1) return `$${Number.isInteger(cost) ? cost : cost.toFixed(2)}`;
  return `$${cost.toFixed(2)}`;
}

