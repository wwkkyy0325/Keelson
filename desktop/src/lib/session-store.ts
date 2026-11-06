import { writable } from "svelte/store";
import {
  initialSession,
  reduce,
  type Action,
  type SessionState,
} from "./session";

export const session = writable<SessionState>(initialSession());

export function dispatch(action: Action): void {
  session.update((state) => reduce(state, action));
}

export function resetSession(): void {
  session.set(initialSession());
}
