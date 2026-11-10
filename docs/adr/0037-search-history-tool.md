# ADR 0037 — `search_history` reads the stored turns

## Status

Accepted and implemented in the engine.

## Context

The rolling summary and the covered index are lossy views of old history.
Passive recall is lexical and only fires when the new input shares terms with
an absorbed segment; the model has no way to ask. The raw turn rows already
live in the session document, so the information exists but is unreachable
from inside a turn.

## Decision

1. A host tool `search_history` (ReadOnly, no permission request) searches
   the in-memory stored turns of the open conversation. It scores turns by
   distinct term hits — ASCII words, CJK bigrams — and returns up to eight
   turns, strongest and newest first, with bounded input and answer excerpts
   and the tool names each turn used.
2. The engine shares the session document with the tool through an
   `Arc<Mutex<StoredSession>>`. The tool sees prior turns only: the running
   turn is persisted when it ends.
3. It is a Keelson tool, not a kernel tool. Every coding profile lists it in
   `tools.visible`; the kernel contract and `keel-*` version are untouched.

## Consequences

- The model can retrieve an old decision, path or command on demand instead
  of trusting a summary that may have dropped it.
- The search is deterministic and dependency-free: no embeddings, no index
  file, no network. CJK queries use overlapping bigrams because the query has
  no word boundaries.
- A hit costs a normal tool call and its output tokens. A miss says so and
  costs one round.
