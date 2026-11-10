// The wire types shared with desktop/src-tauri. Every field is camelCase.

export type ToolLine = {
  name: string;
  ok: boolean | null;
  durationMs: number;
  args: string | null;
  output: string | null;
};

export type VerificationEvidence = {
  itemId: string;
  verifier: string;
  outcome: string;
  detail: string;
};

export type Verification = {
  passed: boolean;
  evidence: VerificationEvidence[];
};

export type TurnUsage = {
  inputTokens: number;
  outputTokens: number;
  cachedInputTokens: number;
};

export type Turn = {
  seq: number;
  input: string;
  answer: string;
  /// The model's thinking for this turn, bounded by the engine. Empty for
  /// models that do not think and for sessions written before the field.
  thinking: string;
  state: string;
  error: string | null;
  durationMs: number;
  /// Summed over the turn's model calls; zeros for sessions written before
  /// the field existed.
  usage: TurnUsage;
  verificationPassed: boolean | null;
  verification: Verification | null;
  tools: ToolLine[];
};

/// One file a turn changed, from its checkpoint pair.
export type FileChange = {
  path: string;
  status: string;
  added: number;
  removed: number;
};

export type TurnPage = {
  turns: Turn[];
  nextBefore: number | null;
};

export type SessionMeta = {
  id: string;
  title: string;
  archived: boolean;
  rig: string;
  createdAt: number;
  updatedAt: number;
  turns: number;
};

/// Context pressure at the end of one turn.
export type ContextPoint = {
  seq: number;
  used: number;
  ceiling: number;
};

/// What the composer's status line shows.
export type SessionStatus = {
  permission: string;
  rig: string;
  model: string;
  provider: string;
  providerName: string;
  contextBudget: number;
  contextWindow: number | null;
  recent: ContextPoint[];
};

export type ProjectEntry = {
  path: string;
  name: string;
  openedAt: number;
};

export type SessionView = {
  rig: string;
  workspace: string;
  sessionId: string;
  turns: Turn[];
  nextBefore: number | null;
  sessions: SessionMeta[];
  projects: ProjectEntry[];
};

export type SetupState = {
  onboarded: boolean;
  hasKey: boolean;
  provider: string;
  providerId: string;
  providerName: string;
  baseUrl: string;
  model: string;
  locale: string;
  lastWorkspace: string;
  keyProvider: string;
  keyProviders: string[];
  manual?: { provider: string; baseUrl: string; model: string } | null;
  /// A SearXNG base URL for models whose provider has no search endpoint.
  searchUrl?: string | null;
  rig: string;
  rigs: { name: string; title: string; description: string }[];
  problems: string[];
};

export type CatalogModel = {
  id: string;
  name: string;
  context: number | null;
  output: number | null;
  costInput: number | null;
  costOutput: number | null;
  reasoning: boolean;
  vision: boolean;
  pdf: boolean;
  releaseDate: string | null;
  updated: string | null;
};

export type CatalogProvider = {
  id: string;
  name: string;
  kind: string;
  baseUrl: string;
  keyEnv: string;
  models: CatalogModel[];
};

export type ModelCatalog = {
  source: "models.dev" | "cache" | "fallback";
  fetchedAt: number;
  stale: boolean;
  providers: CatalogProvider[];
};

export type InteractionOption = {
  id: string;
  label: string;
};

export type Interaction = {
  id: string;
  kind: string;
  prompt: string;
  options: InteractionOption[];
  recommended?: string | null;
};

/// One tool call as the running turn reports it, before the turn is stored.
export type LiveTool = {
  callId: number;
  name: string;
  args: string | null;
  ok: boolean | null;
  durationMs: number;
  error: string | null;
  output: string | null;
};

export type TurnStarted = { seq: number; input: string };
export type TextDelta = { seq: number; text: string };
export type ThinkingDelta = { seq: number; text: string };
export type ToolStartedEvent = {
  seq: number;
  callId: number;
  name: string;
  args: string | null;
};
export type ToolFinishedEvent = {
  seq: number;
  callId: number;
  name: string;
  ok: boolean;
  durationMs: number;
  error: string | null;
  output: string | null;
};
export type TurnChanges = { seq: number };
export type TurnFinished = { turn: Turn };
export type TurnFailed = { seq: number; message: string };
export type SessionUpdated = { meta: SessionMeta };
