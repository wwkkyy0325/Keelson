import type {
  FileChange,
  Interaction,
  LiveBackground,
  LiveTool,
  ProjectEntry,
  SessionMeta,
  SessionView,
  Turn,
  TurnPage,
  TurnUsage,
} from "./protocol";

export type RunningTurn = {
  seq: number;
  input: string;
  /// When the client saw the turn start; the live card counts from here.
  startedAt: number;
  /// Usage summed over the model calls that finished so far.
  usage: TurnUsage;
  /// Model calls finished so far.
  modelCalls: number;
  /// Text from the model call in progress. When that call asks for a tool,
  /// its text was commentary, so it moves to `process` and the answer area
  /// only ever holds the call that actually finished the turn.
  text: string;
  /// Thinking streamed so far, shown while the turn runs.
  thinking: string;
  /// Commentary from calls that went on to call a tool, oldest first.
  process: string[];
  /// Tool calls reported by the running turn, in start order.
  tools: LiveTool[];
  /// Acceptance checks the turn is running, or null until verification starts.
  verifying: number | null;
};

export type SessionState = {
  rig: string;
  workspace: string;
  sessionId: string;
  /** Conversations in the project, newest first. */
  sessions: SessionMeta[];
  /** Recently opened projects, newest first. */
  projects: ProjectEntry[];
  /** Finished turns, ascending by seq. */
  turns: Turn[];
  running: RunningTurn | null;
  /// Background commands of the session. They outlive the turn that started
  /// them; the engine drops one when it hands the result to the model.
  background: LiveBackground[];
  pendingInteraction: Interaction | null;
  /** Cursor for the page above `turns`; null means the start is loaded. */
  nextBefore: number | null;
  loadingOlder: boolean;
  /** A backend failure that did not produce a persisted turn. */
  lastError: string | null;
  /// Bumped on every session-opened, so views that read the engine (status,
  /// change list) reload even when the id stays the same after a reconnect.
  epoch: number;
  /// Bumped when a finished turn's checkpoint pair is recorded, so the change
  /// list reloads without waiting for the snapshot before the turn is shown.
  changesEpoch: number;
  /// The turn whose checkpoint pair just arrived; the diff belongs to it even
  /// if a newer turn has started by the time it loads.
  changedSeq: number | null;
};

export function initialSession(): SessionState {
  return {
    rig: "",
    workspace: "",
    sessionId: "",
    sessions: [],
    projects: [],
    turns: [],
    running: null,
    background: [],
    pendingInteraction: null,
    nextBefore: null,
    loadingOlder: false,
    lastError: null,
    epoch: 0,
    changesEpoch: 0,
    changedSeq: null,
  };
}

export type Action =
  | { type: "session-opened"; view: SessionView }
  | { type: "sessions-loaded"; sessions: SessionMeta[] }
  | { type: "session-updated"; meta: SessionMeta }
  | { type: "turns-loaded"; page: TurnPage }
  | { type: "loading-older"; value: boolean }
  | { type: "turn-started"; seq: number; input: string }
  | { type: "text-delta"; seq: number; text: string }
  | { type: "thinking-delta"; seq: number; text: string }
  | { type: "tool-started"; seq: number; callId: number; name: string; args: string | null }
  | {
      type: "tool-finished";
      seq: number;
      callId: number;
      name: string;
      ok: boolean;
      durationMs: number;
      error: string | null;
      output: string | null;
    }
  | { type: "background-started"; id: string; program: string }
  | {
      type: "background-finished";
      id: string;
      program: string;
      exitCode: number | null;
      durationMs: number;
      outputPath: string;
    }
  | { type: "background-synced"; tasks: LiveBackground[] }
  | {
      type: "model-finished";
      seq: number;
      inputTokens: number;
      outputTokens: number;
      cachedInputTokens: number;
    }
  | { type: "turn-verifying"; seq: number; checks: number }
  | { type: "turn-changes"; seq: number }
  | { type: "turn-changes-loaded"; seq: number; changes: FileChange[] }
  | { type: "turn-finished"; turn: Turn }
  | { type: "turn-failed"; seq: number; message: string }
  | { type: "interaction-requested"; interaction: Interaction }
  | { type: "interaction-resolved"; id: string }
  | { type: "reset" };

/// True when a poll reports the rows already on screen. A finished row keeps
/// its frozen runtime, so an idle poll compares equal and dispatches nothing.
function sameBackground(
  left: LiveBackground[],
  right: LiveBackground[],
): boolean {
  if (left.length !== right.length) return false;
  return left.every((task, index) => {
    const other = right[index];
    return (
      task.id === other.id &&
      task.command === other.command &&
      task.ok === other.ok &&
      task.durationMs === other.durationMs &&
      task.outputPath === other.outputPath
    );
  });
}

export function reduce(state: SessionState, action: Action): SessionState {
  switch (action.type) {
    case "session-opened":
      return {
        ...initialSession(),
        epoch: state.epoch + 1,
        rig: action.view.rig,
        workspace: action.view.workspace,
        sessionId: action.view.sessionId,
        sessions: sortSessions(action.view.sessions),
        projects: action.view.projects,
        turns: normalize(action.view.turns),
        nextBefore: action.view.nextBefore,
      };

    case "sessions-loaded":
      return { ...state, sessions: sortSessions(action.sessions) };

    case "session-updated":
      return {
        ...state,
        sessions: sortSessions([
          ...state.sessions.filter((item) => item.id !== action.meta.id),
          action.meta,
        ]),
      };

    case "turns-loaded":
      return {
        ...state,
        turns: normalize([...action.page.turns, ...state.turns]),
        nextBefore: action.page.nextBefore,
        loadingOlder: false,
      };

    case "loading-older":
      return { ...state, loadingOlder: action.value };

    case "turn-started":
      // A duplicate start after the turn was persisted must not resurrect it.
      if (state.turns.some((turn) => turn.seq === action.seq)) {
        return state;
      }
      return {
        ...state,
        running: {
          seq: action.seq,
          input: action.input,
          startedAt: Date.now(),
          usage: { inputTokens: 0, outputTokens: 0, cachedInputTokens: 0 },
          modelCalls: 0,
          text: "",
          thinking: "",
          process: [],
          tools: [],
          verifying: null,
        },
        pendingInteraction: null,
        lastError: null,
      };

    case "text-delta":
      if (state.running?.seq !== action.seq) {
        return state;
      }
      return {
        ...state,
        running: { ...state.running, text: state.running.text + action.text },
      };

    case "thinking-delta":
      if (state.running?.seq !== action.seq) {
        return state;
      }
      return {
        ...state,
        running: {
          ...state.running,
          thinking: state.running.thinking + action.text,
        },
      };

    case "model-finished":
      if (state.running?.seq !== action.seq) {
        return state;
      }
      return {
        ...state,
        running: {
          ...state.running,
          modelCalls: state.running.modelCalls + 1,
          usage: {
            inputTokens:
              state.running.usage.inputTokens + action.inputTokens,
            outputTokens:
              state.running.usage.outputTokens + action.outputTokens,
            cachedInputTokens:
              state.running.usage.cachedInputTokens + action.cachedInputTokens,
          },
        },
      };

    case "tool-started": {
      if (state.running?.seq !== action.seq) {
        return state;
      }
      // A call that goes on to use a tool was not the answer. Park its text
      // with the tool activity so the answer area stays clean.
      const process = state.running.text.trim()
        ? [...state.running.process, state.running.text]
        : state.running.process;
      const tools = state.running.tools.filter(
        (tool) => tool.callId !== action.callId,
      );
      tools.push({
        callId: action.callId,
        name: action.name,
        args: action.args,
        ok: null,
        durationMs: 0,
        error: null,
        output: null,
      });
      return {
        ...state,
        running: { ...state.running, tools, text: "", process },
      };
    }

    case "tool-finished": {
      if (state.running?.seq !== action.seq) {
        return state;
      }
      const finished = {
        callId: action.callId,
        name: action.name,
        args: null,
        ok: action.ok,
        durationMs: action.durationMs,
        error: action.error,
        output: action.output,
      };
      const known = state.running.tools.some(
        (tool) => tool.callId === action.callId,
      );
      const tools = known
        ? state.running.tools.map((tool) =>
            tool.callId === action.callId ? { ...tool, ...finished, args: tool.args } : tool,
          )
        : [...state.running.tools, finished];
      return { ...state, running: { ...state.running, tools } };
    }

    case "background-started": {
      if (state.background.some((task) => task.id === action.id)) {
        return state;
      }
      const task: LiveBackground = {
        id: action.id,
        program: action.program,
        command: "",
        ok: null,
        durationMs: 0,
        outputPath: null,
      };
      return { ...state, background: [...state.background, task] };
    }

    case "background-finished": {
      const ok = action.exitCode === 0;
      const known = state.background.some((task) => task.id === action.id);
      const background = known
        ? state.background.map((task) =>
            task.id === action.id
              ? {
                  ...task,
                  ok,
                  durationMs: action.durationMs,
                  outputPath: action.outputPath,
                }
              : task,
          )
        : [
            ...state.background,
            {
              id: action.id,
              program: action.program,
              command: "",
              ok,
              durationMs: action.durationMs,
              outputPath: action.outputPath,
            },
          ];
      return { ...state, background };
    }

    case "background-synced":
      // The engine's measured runtime is the row's only clock: the poll
      // refreshes it every second and a local ticker would only drift from
      // it or starve against it. A poll that reports what is already on
      // screen changes nothing: the store replaces its value on every
      // dispatch, so re-dispatching an identical list woke every view.
      if (sameBackground(state.background, action.tasks)) {
        return state;
      }
      return { ...state, background: action.tasks };

    case "turn-verifying":
      if (state.running?.seq !== action.seq) {
        return state;
      }
      return {
        ...state,
        running: { ...state.running, verifying: action.checks },
      };

    case "turn-changes":
      // The pair arrives after the turn row; only the conversation that shows
      // it needs to reload.
      if (!state.turns.some((turn) => turn.seq === action.seq)) {
        return state;
      }
      return {
        ...state,
        changesEpoch: state.changesEpoch + 1,
        changedSeq: action.seq,
      };

    case "turn-changes-loaded":
      // Keep the diff on the turn that made it, so it survives the next turn.
      return {
        ...state,
        turns: state.turns.map((turn) =>
          turn.seq === action.seq ? { ...turn, changes: action.changes } : turn,
        ),
      };

    case "turn-finished":
      return {
        ...state,
        turns: normalize([
          ...state.turns.filter((turn) => turn.seq !== action.turn.seq),
          action.turn,
        ]),
        running:
          state.running?.seq === action.turn.seq ? null : state.running,
        pendingInteraction: null,
        lastError: null,
      };

    case "turn-failed":
      return {
        ...state,
        running:
          state.running?.seq === action.seq ? null : state.running,
        pendingInteraction: null,
        lastError: action.message,
      };

    case "interaction-requested":
      return { ...state, pendingInteraction: action.interaction };

    case "interaction-resolved":
      return {
        ...state,
        pendingInteraction:
          state.pendingInteraction?.id === action.id
            ? null
            : state.pendingInteraction,
      };

    case "reset":
      return initialSession();
  }
}

/// Newest first, with a stable fallback so two rows written in the same
/// millisecond do not swap places between renders.
function sortSessions(sessions: SessionMeta[]): SessionMeta[] {
  return [...sessions].sort(
    (left, right) =>
      right.updatedAt - left.updatedAt ||
      right.createdAt - left.createdAt ||
      right.id.localeCompare(left.id),
  );
}

/// Deduplicates by turn sequence and keeps the transcript ascending. Pages and
/// live events may overlap; seq is the only identity.
function normalize(turns: Turn[]): Turn[] {
  const bySeq = new Map<number, Turn>();
  for (const turn of turns) {
    bySeq.set(turn.seq, turn);
  }
  return [...bySeq.values()].sort((a, b) => a.seq - b.seq);
}
