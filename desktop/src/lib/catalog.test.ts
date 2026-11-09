import { describe, expect, it } from "vitest";
import {
  defaultSelection,
  findSelection,
  formatContext,
  formatPrice,
  modelsFor,
} from "./catalog";
import type { ModelCatalog, SetupState } from "./protocol";

const catalog: ModelCatalog = {
  source: "models.dev",
  fetchedAt: 1_760_000_000,
  stale: false,
  providers: [
    {
      id: "anthropic",
      name: "Anthropic",
      kind: "anthropic-compatible",
      baseUrl: "https://api.anthropic.com",
      keyEnv: "ANTHROPIC_API_KEY",
      models: [
        {
          id: "claude-sonnet-5-5",
          name: "Claude Sonnet 5.5",
          context: 1_000_000,
          output: 128_000,
          costInput: 3,
          costOutput: 15,
          reasoning: true,
          vision: true,
          pdf: true,
          releaseDate: "2026-09-28",
          updated: null,
        },
      ],
    },
    {
      id: "deepseek",
      name: "DeepSeek",
      kind: "openai-compatible",
      baseUrl: "https://api.deepseek.com",
      keyEnv: "DEEPSEEK_API_KEY",
      models: [
        {
          id: "deepseek-flash",
          name: "DeepSeek Flash",
          context: 1_048_576,
          output: 65_536,
          costInput: 0.28,
          costOutput: 0.42,
          reasoning: true,
          vision: true,
          pdf: false,
          releaseDate: "2026-10-17",
          updated: null,
        },
      ],
    },
  ],
};

function setup(overrides: Partial<SetupState> = {}): SetupState {
  return {
    onboarded: false,
    hasKey: false,
    provider: "openai-compatible",
    providerId: "deepseek",
    providerName: "DeepSeek",
    baseUrl: "https://api.deepseek.com",
    model: "deepseek-flash",
    locale: "zh",
    lastWorkspace: "",
    keyProvider: "",
    keyProviders: [],
    rig: "coding-live",
    rigs: [],
    problems: [],
    ...overrides,
  };
}

describe("catalog helpers", () => {
  it("finds a saved provider and model", () => {
    expect(findSelection(catalog, "deepseek", "deepseek-flash")?.model.id).toBe(
      "deepseek-flash",
    );
    expect(findSelection(catalog, "deepseek", "missing")).toBeNull();
  });

  it("falls back to the default provider only on a first run", () => {
    expect(defaultSelection(catalog, setup({ model: "missing" }))?.model.id).toBe(
      "deepseek-flash",
    );
    expect(
      defaultSelection(catalog, setup({ model: "missing" }), false),
    ).toBeNull();
  });

  it("filters by provider and query", () => {
    expect(modelsFor(catalog, "all", "claude")).toHaveLength(1);
    expect(modelsFor(catalog, "deepseek", "")).toHaveLength(1);
    expect(modelsFor(catalog, "anthropic", "deepseek")).toHaveLength(0);
  });

  it("formats context and price compactly", () => {
    expect(formatContext(1_048_576)).toBe("1M");
    expect(formatContext(131_072)).toBe("131K");
    expect(formatContext(null)).toBe("—");
    expect(formatPrice(3)).toBe("$3");
    expect(formatPrice(0.28)).toBe("$0.28");
    expect(formatPrice(null)).toBe("—");
  });
});
