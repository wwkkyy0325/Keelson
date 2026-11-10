<script lang="ts">
  import { t } from "../lib/i18n";
  import type { FileChange, Turn } from "../lib/protocol";
  import Markdown from "./Markdown.svelte";

  let {
    turn,
    isLatest = false,
    changes = [],
    undoable = false,
    onUndo = () => {},
    onBranch = () => {},
  }: {
    turn: Turn;
    /// Undo belongs to the latest turn only; the icon still shows on it when
    /// there is nothing to undo, greyed out.
    isLatest?: boolean;
    changes?: FileChange[];
    undoable?: boolean;
    onUndo?: () => void;
    onBranch?: () => void;
  } = $props();

  let passed = $derived(turn.verification?.passed ?? turn.verificationPassed);
  let copied = $state<string | null>(null);
  let added = $derived(changes.reduce((total, change) => total + change.added, 0));
  let removed = $derived(
    changes.reduce((total, change) => total + change.removed, 0),
  );

  async function copy(text: string, key: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
      copied = key;
      setTimeout(() => {
        if (copied === key) copied = null;
      }, 1500);
    } catch {
      // The clipboard can be unavailable; the text is still on screen.
    }
  }

  function copyTip(key: string): string {
    return copied === key ? $t("turn.copied") : $t("turn.copy");
  }

  function mark(ok: boolean | null): string {
    return ok === false ? "✗" : ok === true ? "✓" : "·";
  }

  /// The kernel records whole milliseconds; an instant tool reports 0. Show
  /// that as `<1 ms` rather than an empty column, which reads as missing data.
  function took(ms: number, ok: boolean | null): string {
    if (ok === null) return "";
    if (ms <= 0) return "<1 ms";
    return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;
  }

  let usage = $derived(turn.usage);
  let showUsage = $derived(
    usage.inputTokens > 0 || usage.outputTokens > 0,
  );
  let cacheShare = $derived(
    usage.inputTokens > 0
      ? Math.round((usage.cachedInputTokens / usage.inputTokens) * 100)
      : 0,
  );

  /// Small enough to sit beside the actions, precise enough to compare turns.
  function compactCount(count: number): string {
    if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
    if (count >= 100_000) return `${Math.round(count / 1000)}k`;
    if (count >= 1000) return `${(count / 1000).toFixed(1)}k`;
    return String(count);
  }

  function turnTime(ms: number): string {
    if (ms < 1000) return `${ms} ms`;
    const seconds = ms / 1000;
    if (seconds < 60) return `${seconds.toFixed(1)} s`;
    const minutes = Math.floor(seconds / 60);
    const rest = Math.round(seconds % 60);
    return `${minutes}m ${String(rest).padStart(2, "0")}s`;
  }
</script>

<article class="turn">
  <div class="turn-block">
    <div class="user">{turn.input}</div>
    <div class="turn-actions user-actions">
      <div class="turn-buttons">
        <button
          class="turn-action"
          class:copied={copied === "input"}
          aria-label={copyTip("input")}
          data-tip={copyTip("input")}
          onclick={() => copy(turn.input, "input")}
        >
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <rect x="9" y="9" width="11" height="11" rx="2" />
            <path d="M5 15V5a2 2 0 0 1 2-2h10" />
          </svg>
        </button>
      </div>
    </div>
  </div>

  <!-- Thinking and tool calls share one indented group: both are how the
       turn got to the answer, collapsed until asked for. -->
  {#if turn.thinking || turn.tools.length > 0}
    <div class="tools">
      {#if turn.thinking}
        <details class="tool">
          <summary>
            <span class="tool-mark">·</span>
            <span class="tool-name">{$t("turn.thinking")}</span>
            <span class="tool-time"
              >{$t("turn.thinkingChars", {
                count: compactCount(turn.thinking.length),
              })}</span
            >
          </summary>
          <div class="tool-detail">
            <pre>{turn.thinking}</pre>
          </div>
        </details>
      {/if}
      {#each turn.tools as tool}
        <details class="tool">
          <summary>
            <span
              class:fail={tool.ok === false}
              class:ok={tool.ok === true}
              class:pending={tool.ok === null}
              class="tool-mark">{mark(tool.ok)}</span
            >
            <span class="tool-name">{tool.name}</span>
            {#if tool.args}
              <span class="tool-args">{tool.args}</span>
            {/if}
            {#if tool.ok !== null}
              <span class="tool-time">{took(tool.durationMs, tool.ok)}</span>
            {/if}
          </summary>
          {#if tool.args}
            <div class="tool-detail">
              <span class="tool-label">{$t("turn.toolArgs")}</span>
              <pre>{tool.args}</pre>
            </div>
          {/if}
          {#if tool.output}
            <div class="tool-detail">
              <span class="tool-label">{$t("turn.toolOutput")}</span>
              <pre>{tool.output}</pre>
            </div>
          {/if}
        </details>
      {/each}
    </div>
  {/if}

  {#if turn.answer || isLatest}
    <div class="turn-block">
      {#if turn.answer}
        <div class="answer"><Markdown source={turn.answer} /></div>
      {/if}
      {#if changes.length > 0 || passed !== null}
        <div class="turn-result">
          {#if changes.length > 0}
            <div class="changes">
              <details>
                <summary>
                  <span class="change-count">
                    {$t("turn.changedFiles", { count: changes.length })}
                  </span>
                  <span class="change-add">+{added}</span>
                  <span class="change-del">−{removed}</span>
                </summary>
                {#each changes as change}
                  <div class="change-row">
                    <span class="change-status">{change.status}</span>
                    <span class="change-path" title={change.path}>{change.path}</span>
                    <span class="change-add">+{change.added}</span>
                    <span class="change-del">−{change.removed}</span>
                  </div>
                {/each}
              </details>
            </div>
          {/if}
          {#if passed !== null}
            <div class="verification" class:passed={passed}>
              <span>{passed ? "✓" : "✗"}</span>
              <span>
                {passed ? $t("app.verified") : $t("app.verificationFailed")}
              </span>
            </div>
          {/if}
        </div>
        {#if turn.verification?.evidence.length}
          <details class="verification-detail">
            <summary>{$t("app.verificationEvidence")}</summary>
            {#each turn.verification.evidence as evidence}
              <div class="verification-item">
                <span class="dim">{evidence.itemId} · {evidence.outcome}</span>
                <pre>{evidence.detail}</pre>
              </div>
            {/each}
          </details>
        {/if}
      {/if}
      <div class="turn-actions">
        <div class="turn-buttons">
          {#if turn.answer}
            <button
              class="turn-action"
              class:copied={copied === "answer"}
              aria-label={copyTip("answer")}
              data-tip={copyTip("answer")}
              onclick={() => copy(turn.answer, "answer")}
            >
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <rect x="9" y="9" width="11" height="11" rx="2" />
                <path d="M5 15V5a2 2 0 0 1 2-2h10" />
              </svg>
            </button>
            <button
              class="turn-action"
              aria-label={$t("turn.branch")}
              data-tip={$t("turn.branch")}
              onclick={onBranch}
            >
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <line x1="6" y1="3" x2="6" y2="15" />
                <circle cx="18" cy="6" r="3" />
                <circle cx="6" cy="18" r="3" />
                <path d="M18 9a9 9 0 0 1-9 9" />
              </svg>
            </button>
          {/if}
          {#if isLatest}
            <button
              class="turn-action"
              aria-label={undoable ? $t("app.undoTurn") : $t("turn.nothingToUndo")}
              data-tip={undoable ? $t("app.undoTurn") : $t("turn.nothingToUndo")}
              disabled={!undoable}
              onclick={onUndo}
            >
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <polyline points="1 4 1 10 7 10" />
                <path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10" />
              </svg>
            </button>
          {/if}
        </div>
        <div class="turn-meta">
          {#if turn.durationMs > 0}
            <span title={$t("turn.durationHint")}>
              {turnTime(turn.durationMs)}
            </span>
          {/if}
          {#if showUsage}
            <span title={$t("turn.inputTokensHint")}>
              ↑{compactCount(usage.inputTokens)}
            </span>
            <span title={$t("turn.outputTokensHint")}>
              ↓{compactCount(usage.outputTokens)}
            </span>
            {#if usage.cachedInputTokens > 0}
              <span title={$t("turn.cacheHint")}>
                {$t("turn.cacheShare", { percent: cacheShare })}
              </span>
            {/if}
          {/if}
        </div>
      </div>
    </div>
  {/if}

  {#if turn.state !== "Done" && !turn.answer}
    <div class="state">{turn.state}</div>
  {/if}
  {#if turn.error}
    <div class="error">{turn.error}</div>
  {/if}
</article>
