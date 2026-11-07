import type {
  Interaction,
  ProjectEntry,
  SessionMeta,
  SessionView,
  Turn,
  TurnPage,
} from "./protocol";

export type RunningTurn = {
  seq: number;
  input: string;
  text: string;
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
  pendingInteraction: Interaction | null;
  /** Cursor for the page above `turns`; null means the start is loaded. */
  nextBefore: number | null;
  loadingOlder: boolean;
  /** A backend failure that did not produce a persisted turn. */
  lastError: string | null;
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
    pendingInteraction: null,
    nextBefore: null,
    loadingOlder: false,
    lastError: null,
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
  | { type: "turn-finished"; turn: Turn }
  | { type: "turn-failed"; seq: number; message: string }
  | { type: "interaction-requested"; interaction: Interaction }
  | { type: "interaction-resolved"; id: string }
  | { type: "clear-error" }
  | { type: "reset" };

export function reduce(state: SessionState, action: Action): SessionState {
  switch (action.type) {
    case "session-opened":
      return {
        ...initialSession(),
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
        running: { seq: action.seq, input: action.input, text: "" },
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

    case "turn-finished":
      return {
        ...state,
        turns: normalize([
          ...state.turns.filter((turn) => turn.seq !== action.turn.seq),
          action.turn,
        ]),
        running:
          state.running?.seq === action.turn.seq ? null : state.running,
        lastError: null,
      };

    case "turn-failed":
      return {
        ...state,
        running:
          state.running?.seq === action.seq ? null : state.running,
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

    case "clear-error":
      return { ...state, lastError: null };

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
