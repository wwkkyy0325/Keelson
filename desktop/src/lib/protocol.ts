// The wire types shared with desktop/src-tauri. Every field is camelCase.

export type ToolLine = {
  name: string;
  ok: boolean | null;
  durationMs: number;
  args: string | null;
  output: string | null;
  /// True when an identical call earlier in the same turn answered this one,
  /// so no command ran. Absent on turns stored before the field.
  reused?: boolean;
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
  /// Model calls spent in this turn; recovered from events for old sessions.
  modelCalls: number;
  verificationPassed: boolean | null;
  verification: Verification | null;
  tools: ToolLine[];
  /// Files this turn changed, from its checkpoint pair. Absent on a turn
  /// emitted live before its checkpoint was recorded.
  changes?: FileChange[];
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
  /// Memory entries the `[memory]` block left out of that turn.
  factsClipped: number;
};

/// What the composer's status line shows.
export type SessionStatus = {
  /// The conversation this answer describes; a late answer for another one is
  /// dropped rather than painted over the current conversation.
  sessionId: string;
  /// The persisted setting.
  permission: string;
  /// The level actually in force; differs while a full-auto grant is live.
  permissionEffective: string;
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

/// One rig file the host found, as setup and the status line show it.
export type RigOption = {
  name: string;
  title: string;
  description: string;
};

/// One file copied into the conversation's read-only attachment folder.
export type Attachment = {
  name: string;
  path: string;
  size: number;
};

export type SessionView = {
  rig: string;
  workspace: string;
  sessionId: string;
  /// The turn this conversation is running right now, so switching back to it
  /// shows the live card instead of an empty transcript.
  running: { seq: number; input: string } | null;
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
  modelContext: number | null;
  locale: string;
  lastWorkspace: string;
  keyProvider: string;
  keyProviders: string[];
  manual?: {
    provider: string;
    baseUrl: string;
    model: string;
    context?: number | null;
  } | null;
  /// The keychain slot the current connection reads.
  keySlot: string;
  /// Named setups and the one the current connection came from.
  presets: Preset[];
  activePreset: string | null;
  /// A SearXNG base URL, the last web-search backend after the model's own.
  searchUrl?: string | null;
  /// Where the per-turn records go (ADR 0063): `project`, `state` or `custom`.
  journalMode: string;
  journalPath: string;
  rig: string;
  rigs: RigOption[];
  problems: string[];
};

/// Where the two records of a turn land for the open project: the step log's
/// database and the directory of task documents.
export type RecordPaths = {
  log: string;
  progressDir: string;
};

/// A named setup. It records the key slot, never the key itself, so several
/// presets may share one key and one key may serve several models.
export type Preset = {
  id: string;
  name: string;
  rig: string;
  provider: string;
  providerId: string;
  providerName: string;
  baseUrl: string;
  model: string;
  modelContext: number | null;
  keySlot: string | null;
  permission: string | null;
  searchUrl: string | null;
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
  /// The answer came from an identical call earlier in the same turn.
  reused?: boolean;
};

/// Live events name the conversation they belong to. Several conversations can
/// have a turn running at once, so a view drops the events of the others.
export type EventTag = { sessionId?: string };

export type TurnStarted = EventTag & { seq: number; input: string };
export type TextDelta = EventTag & { seq: number; text: string };
export type ThinkingDelta = EventTag & { seq: number; text: string };
/// One finished model call; the running turn adds it to its live cost line.
export type ModelFinished = EventTag & {
  seq: number;
  inputTokens: number;
  outputTokens: number;
  cachedInputTokens: number;
};
export type ToolStartedEvent = EventTag & {
  seq: number;
  callId: number;
  name: string;
  args: string | null;
};
export type ToolFinishedEvent = EventTag & {
  seq: number;
  callId: number;
  name: string;
  ok: boolean;
  durationMs: number;
  error: string | null;
  output: string | null;
  reused?: boolean;
};
/// A shell command the running turn moved to the background.
export type BackgroundStarted = EventTag & {
  seq: number;
  id: string;
  program: string;
  pid: number;
};
export type BackgroundFinished = EventTag & {
  seq: number;
  id: string;
  program: string;
  exitCode: number | null;
  durationMs: number;
  outputPath: string;
  /// The user stopped it; a non-zero exit code is the stop, not a failure.
  stopped: boolean;
};

/// One background command as the session shows it. `durationMs` is the
/// engine's measured runtime, refreshed by the poll, and the only clock the
/// row displays.
export type LiveBackground = {
  id: string;
  program: string;
  /// The whole command line; empty until a poll reports it.
  command: string;
  ok: boolean | null;
  durationMs: number;
  outputPath: string | null;
  /// The user asked to stop it. While the command still runs the row says so
  /// and stops offering the control.
  stopped: boolean;
};

/// The engine's view of a background command, without consuming its result.
export type BackgroundTaskView = {
  id: string;
  program: string;
  /// The whole command line, so the row shows what runs, not the program.
  command: string;
  running: boolean;
  exitCode: number | null;
  durationMs: number;
  outputPath: string;
  /// The user stopped it. While it still runs the row says so and offers no
  /// control.
  stopped: boolean;
};

/// The answer is written; the turn is running its acceptance checks.
export type TurnVerifying = EventTag & { seq: number; checks: number };
export type TurnChanges = EventTag & { seq: number };
export type TurnFinished = EventTag & { turn: Turn };
export type TurnFailed = EventTag & { seq: number; message: string };
export type SessionUpdated = { meta: SessionMeta };
