// The wire types shared with desktop/src-tauri. Every field is camelCase.

export type ToolLine = {
  name: string;
  ok: boolean | null;
  durationMs: number;
  args: string | null;
  output: string | null;
};

export type Turn = {
  seq: number;
  input: string;
  answer: string;
  state: string;
  error: string | null;
  durationMs: number;
  tools: ToolLine[];
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
};

export type TurnStarted = { seq: number; input: string };
export type TextDelta = { seq: number; text: string };
export type TurnFinished = { turn: Turn };
export type TurnFailed = { seq: number; message: string };
export type SessionUpdated = { meta: SessionMeta };
