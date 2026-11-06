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

export type SessionView = {
  rig: string;
  workspace: string;
  sessionId: string;
  turns: Turn[];
  nextBefore: number | null;
};

export type SetupState = {
  onboarded: boolean;
  hasKey: boolean;
  provider: string;
  baseUrl: string;
  model: string;
  locale: string;
  lastWorkspace: string;
  problems: string[];
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
