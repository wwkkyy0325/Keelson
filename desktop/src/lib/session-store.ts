import { get, writable, type Readable, type Writable } from "svelte/store";
import { perf } from "./perf";
import type { SessionView } from "./protocol";
import {
  initialSession,
  reduce,
  type Action,
  type SessionState,
} from "./session";

/// How many conversations the window keeps state for.
///
/// The state here is a cache of what the engine and the disk said, never the
/// source of truth: a conversation dropped from it is one `open` away from being
/// whole again, because the engine still holds its turn and the document still
/// holds its transcript. That is what lets the bound be a plain number instead
/// of a watchdog: nothing has to notice that a conversation was forgotten and
/// put it back, because forgetting costs one open and only when it is opened.
export const KEEP = 8;

type Entry = {
  store: Writable<SessionState>;
  /// Order of last use; the least recently used idle conversation goes first.
  used: number;
};

const entries = new Map<string, Entry>();
let stamps = 0;

/// The conversation the window draws. Every other entry keeps receiving its own
/// events: a turn the user switched away from goes on writing its transcript
/// into its own state, so coming back finds it whole instead of blank until a
/// reload.
export const focus = writable<string>("");

/// Conversations with a turn, or a background command, still in flight.
///
/// This is what one window with one conversation never had to show, and the
/// only thing the rail adds: work that continues while the window looks
/// elsewhere. It is derived from each conversation's own state on every write,
/// so it is never out of step with what is drawn — no timer reconciles them.
export const busy = writable<ReadonlySet<string>>(new Set());

/// Actions that rewrite the project's conversation list rather than one
/// conversation's transcript. Each entry carries its own copy of that list, so
/// these go to all of them: two copies that disagreed would leave the rail
/// showing the new title in one conversation and the old one in another.
const PROJECT_WIDE: ReadonlySet<Action["type"]> = new Set([
  "sessions-loaded",
  "session-updated",
]);

function entryFor(id: string): Entry {
  let entry = entries.get(id);
  if (!entry) {
    entry = { store: writable(initialSession()), used: 0 };
    entries.set(id, entry);
    evict(id);
  }
  entry.used = ++stamps;
  return entry;
}

/// Drops the least recently used idle conversation once the window holds more
/// than `KEEP`. The conversation on screen and the ones still working are never
/// candidates: dropping one of those throws away a stream the user is watching,
/// which is the failure this cache must not introduce.
function evict(keep: string): void {
  while (entries.size > KEEP) {
    let victim: string | null = null;
    let oldest = Number.POSITIVE_INFINITY;
    for (const [id, entry] of entries) {
      if (id === keep || id === get(focus)) continue;
      if (inFlight(entry.store)) continue;
      if (entry.used < oldest) {
        oldest = entry.used;
        victim = id;
      }
    }
    if (victim === null) return;
    entries.delete(victim);
  }
}

/// A conversation whose state cannot be thrown away yet. A background command
/// counts: its rows live in this state until the engine hands the result to the
/// model, and they are the only rows the window has for it.
function inFlight(store: Writable<SessionState>): boolean {
  const state = get(store);
  return (
    state.running !== null || state.background.some((task) => task.ok === null)
  );
}

function write(id: string, action: Action): void {
  const entry = entryFor(id);
  perf.count("action");
  let working = false;
  entry.store.update((state) => {
    const next = reduce(state, action);
    working =
      next.running !== null || next.background.some((task) => task.ok === null);
    return next;
  });
  mark(id, working);
}

/// Keeps the rail's set equal to what the states say, and leaves it untouched
/// when nothing changed: the store replaces its value on every write, and the
/// rail must not re-render on every streamed delta.
function mark(id: string, working: boolean): void {
  busy.update((shown) => {
    if (shown.has(id) === working) return shown;
    const next = new Set(shown);
    if (working) next.add(id);
    else next.delete(id);
    return next;
  });
}

/// Writes to the conversation on screen. Component-local answers — a poll, an
/// undo, a rename — use this; backend events name their conversation and go
/// through `dispatchTo`.
export function dispatch(action: Action): void {
  const id = get(focus);
  if (!PROJECT_WIDE.has(action.type)) {
    write(id, action);
    return;
  }
  for (const key of new Set([...entries.keys(), id])) write(key, action);
}

/// Writes to one named conversation. Backend events arrive here: the id on the
/// payload decides where the action lands, not what the window happens to show.
///
/// An event for a conversation the window holds no state for is dropped rather
/// than given a new state. Opening that conversation reads the engine and the
/// disk, so the only thing lost is the deltas that arrive before the open — and
/// the open answers them.
export function dispatchTo(sessionId: string, action: Action): void {
  if (!entries.has(sessionId)) return;
  write(sessionId, action);
}

/// Opens a conversation and draws it. The view is the whole of what the backend
/// knows about it, so the state is rebuilt from that, not merged with whatever
/// was there before.
export function openConversation(view: SessionView): void {
  focus.set(view.sessionId);
  write(view.sessionId, { type: "session-opened", view });
}

/// The conversation left the project: its state goes with it.
export function forget(sessionId: string): void {
  entries.delete(sessionId);
  mark(sessionId, false);
  if (get(focus) === sessionId) focus.set("");
}

/// The window forgets every conversation without asking the backend to drop
/// anything. For recovery: the engines are rebuilt by the open that follows.
export function closeAll(): void {
  entries.clear();
  busy.set(new Set());
  focus.set("");
}

export function resetSession(): void {
  write(get(focus), { type: "reset" });
}

/// What the window holds. For tests and for the perf report; the window itself
/// has no use for the whole set.
export function held(): string[] {
  return [...entries.keys()];
}

/// The state of the conversation on screen, as a store.
///
/// Components read this exactly as they read the single state before there were
/// several: the seam is here, so a window that keeps N conversations needed no
/// change in any component that draws one. Subscribing to it subscribes to the
/// state of the focused conversation and re-points when the focus moves.
export const session: Readable<SessionState> = {
  subscribe(run: (value: SessionState) => void) {
    let stopInner = () => {};
    const stopFocus = focus.subscribe((id) => {
      stopInner();
      stopInner = entryFor(id).store.subscribe(run);
    });
    return () => {
      stopInner();
      stopFocus();
    };
  },
};
