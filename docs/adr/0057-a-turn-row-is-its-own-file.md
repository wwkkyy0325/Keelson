# 0057 — A turn row is its own file

Status: accepted

Amends 0006.

## Context

ADR 0006 made one JSON document per conversation, with the turn rows inside it.
Every save serialised the whole document and renamed it into place. That is
correct and atomic, and it does not scale: the largest conversation reached
10.5 MB, so each of its turns rewrote 10.5 MB, and the total bytes written grow
with the square of the turn count. The listing path paid too: reconciling the
index parsed every document, turns included, to count them and to derive the
title from the first user message.

The rows were never actually rewritten — a finished turn does not change. The
save rewrote them because they happened to live in the same file as the header,
the kernel snapshot and the title, which do change.

## Decision

- `s-<id>.json` becomes a header: workspace, rig, title, archive flag, the
  kernel snapshot and the todo list, and no turns. Nothing in it grows with the
  length of the conversation.
- Each turn is `s-<id>.turns/<seq:08>.json`, written once and never rewritten.
  A save writes the files that do not exist yet and stops, so the cost of a
  turn is that turn, not the conversation.
- Every file still goes through a temporary name and a rename. A crash between
  the header and a row leaves a conversation missing its newest row, never one
  with a half-written row.
- `Store::load` reads the header, then the rows in sequence order. A document
  written before the split still carries its rows inline; they are read as they
  are and written out one file each on the next save. Nothing has to be
  migrated by hand and an older build can still read a newer document's header.
- `Store::read_document` reads the header alone; `Store::turn_count`
  answers a count from the row file names, and `Store::first_turn` reads one
  row for the title a nameless conversation falls back to. The desktop index
  asks those three questions, so listing a project opens a header per unseen
  document instead of parsing every turn of every conversation.
- `Store::load_unchecked` stays for the caller that needs the rows themselves.
- `Store::delete` removes the header and the row directory together. Deleting
  a conversation cannot leave its rows behind.

## Consequences

- A save is proportional to one turn. A twenty-turn conversation and a
  thousand-turn one cost the same per turn.
- The listing path reads a header per unseen document, plus one row when a
  conversation has no custom title. The index stays a cache and is still
  rebuilt from the directory.
- A directory holds more files: a conversation is one header and *n* rows. The
  per-conversation count is bounded by the turns, which is the thing that
  grows anyway.
- Compaction, export and cross-project search still read the whole
  conversation; none of them is on the per-turn path.
