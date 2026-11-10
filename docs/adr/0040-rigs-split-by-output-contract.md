# ADR 0040 — Rigs are split by output contract

## Status

Accepted.

## Context

M5 added one non-coding rig named `general`, defined by topic ("writing,
research and file work"). Usage studies of assistant traffic put work use
into a few clusters: editing user-supplied text, writing documents, and
seeking information; coding is served by its own rig. Within those clusters
the deliverables differ sharply — a cited answer, an edited document, a
presentation, a video — and so does what counts as done. One prompt cannot
carry four different output contracts, and the kernel gives every rig the
same tools: a rig only chooses which tools exist and what the system prompt
says. Any split therefore has to earn its keep through the artifact and its
verification, not through a domain label.

## Decision

1. A rig is split when its artifact or its verification differs from another
   rig's, not when its subject differs. Two rigs that would share both are one
   rig with a different title.
2. `research` owns the cited answer. Every factual sentence carries a source
   id; the reply ends with a ledger naming the title, URL, what was read (page
   text, search snippet only, or a local file) and the supporting quotation.
   Unsourceable claims are listed as unverified.
3. `office` owns the document. The verification is the file: the requested
   change is the only change, and a deliverable is written to a file rather
   than left in the reply.
4. `coding-live` stays as it was; its artifact and verification are the
   repository's own checks.
5. A deck or a video rig arrives only when its artifact can be verified by
   the host — the file opens, a page renders, a probe reports duration and
   resolution. Until then it is a prompt over `shell`, which is not a rig
   worth selecting.

## Consequences

- The picker offers three rigs, split by what they produce. The old `general`
  name is gone: a rig with no contract is a catch-all that verifies nothing.
- Each rig needs its own small evaluation set; a single shared suite would
  measure the prompt that happens to be running, not the contract.
- A new medium (deck, video) does not get a rig for free. The host must first
  be able to check the artifact, or the rig only produces optimistic prose.

## Alternatives rejected

- One `general` rig with instructions for every medium: contradictory
  priorities and no verification for any of them.
- Topic-named rigs (`writer`, `analyst`, `editor`) that share the document
  contract: several files that differ only in adjectives.
- Splitting by tool set alone: the kernel already scopes tools per rig, but
  without a contract two tool sets drift into the same behaviour.
