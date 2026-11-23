import { writable } from "svelte/store";
import {
  initialSession,
  reduce,
  type Action,
  type SessionState,
} from "./session";

import { perf } from "./perf";

export const session = writable<SessionState>(initialSession());

export function dispatch(action: Action): void {
  perf.count("action");
  session.update((state) => reduce(state, action));
}

export function resetSession(): void {
  session.set(initialSession());
}
