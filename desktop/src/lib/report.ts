/// The window's own account of a turn, written to the host (ADR 0071).
///
/// A turn that holds the window at full CPU leaves nothing behind: the process
/// is sampled after the fact, the page is reloaded to recover, and the evidence
/// dies with it. The account is written while the turn runs and again when it
/// ends, so the next report of "it froze" arrives with the counters that say
/// which path ran away — streamed batches, markdown samples, background polls,
/// missed frames — and how much of the thread was in the page.
///
/// The write is one `invoke` of a few kilobytes, bounded on the host side to the
/// newest forty accounts. It is never awaited by the turn: a diagnostic that
/// makes the turn slower is the thing it was meant to explain.
import { invoke } from "@tauri-apps/api/core";
import { perf, type PerfReport } from "./perf";

/// A turn in flight is accounted this often. Frequent enough to bracket a
/// spike, rare enough that the accounts of one turn stay small.
export const ACCOUNT_EVERY_MS = 5000;

/// What the page was holding when the account was taken. Node counts separate
/// "the transcript grew" from "the transcript was re-rendered".
export type DomShare = {
  /// Every element in the document.
  nodes: number;
  /// Answer bodies currently rendered (the rest are stand-ins).
  bodies: number;
  /// Tool rows currently in the page, folded or not.
  tools: number;
};

export type Account = PerfReport & {
  seq: number;
  phase: string;
  at: number;
  dom: DomShare;
};

function count(selector: string): number {
  if (typeof document === "undefined") return 0;
  return document.querySelectorAll(selector).length;
}

/// The account as it is written to disk.
export function accountNow(seq: number, phase: string): Account {
  const nodes =
    typeof document === "undefined"
      ? 0
      : document.getElementsByTagName("*").length;
  return {
    ...perf.report(),
    seq,
    phase,
    at: Date.now(),
    dom: {
      nodes,
      bodies: count("article.turn .markdown"),
      tools: count(".tool"),
    },
  };
}

/// Writes one account for turn `seq` of `workspace`. Failures are swallowed: the
/// account is evidence, not a step of the turn.
export async function reportTurn(
  workspace: string,
  sessionId: string,
  seq: number,
  phase: string,
): Promise<void> {
  if (workspace === "") return;
  let report: string;
  try {
    report = JSON.stringify(accountNow(seq, phase));
  } catch {
    // A circular or oversized figure would fail here; the turn still runs.
    return;
  }
  try {
    await invoke("perf_report", { workspace, sessionId, seq, phase, report });
  } catch {
    // The host may be shutting down; the next account is written anyway.
  }
}
