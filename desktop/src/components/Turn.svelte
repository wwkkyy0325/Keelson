<script lang="ts">
  import { untrack } from "svelte";
  import { t } from "../lib/i18n";
  import type { FileChange, Turn } from "../lib/protocol";
  import {
    INITIAL_DENSITY,
    cull,
    estimateTurnHeight,
  } from "../lib/cull";
  import Markdown from "./Markdown.svelte";

  let {
    turn,
    isLatest = false,
    changes = [],
    undoable = false,
    onUndo = () => {},
    onBranch = () => {},
    scrollRoot = null,
    density = INITIAL_DENSITY,
    onMeasured = () => {},
    onRendered = () => {},
  }: {
    turn: Turn;
    /// Undo belongs to the latest turn only; the icon still shows on it when
    /// there is nothing to undo, greyed out.
    isLatest?: boolean;
    changes?: FileChange[];
    undoable?: boolean;
    onUndo?: () => void;
    onBranch?: () => void;
    /// The scrolling transcript. A turn far above it is not visible, so its
    /// body waits until it comes near (lib/cull.ts).
    scrollRoot?: HTMLElement | null;
    /// Pixels of rendered answer per character, learned by the transcript, read
    /// once when the stand-in is created.
    density?: number;
    /// Reports this turn's rendered height, so the stand-ins of the turns
    /// above it can be sized from real markup instead of a guess.
    onMeasured?: (chars: number, renderedPx: number) => void;
    /// A stand-in was just replaced by the real body, which changed the height
    /// of everything below it.
    onRendered?: () => void;
  } = $props();

  let passed = $derived(turn.verification?.passed ?? turn.verificationPassed);
  let copied = $state<string | null>(null);
  /// The body is built once the turn nears the viewport. The newest turn is
  /// where the user is looking, so it starts rendered.
  // svelte-ignore state_referenced_locally
  let revealed = $state(isLatest);
  /// The height reserved while the body has not rendered. It is taken once, at
  /// mount, and never recomputed: a figure that keeps changing resizes the
  /// turns above the reader, and the text they are reading moves with it. The
  /// learned figure still reaches the turns that mount later (a page of older
  /// turns), which is where it can help.
  // svelte-ignore state_referenced_locally
  let placeholder = $state(
    estimateTurnHeight(
      turn.answer.length,
      turn.tools.length,
      changes.length > 0 || passed !== null,
      density,
    ),
  );

  let article = $state<HTMLElement | null>(null);
  /// The stand-in this turn is holding right now, so the part of the article
  /// that is never given back can be measured against it.
  let standIn = $state<HTMLElement | null>(null);
  /// The height this body really had when it rendered, article and all. A body
  /// that is given back uses the body part of it as its stand-in, so collapsing
  /// does not resize the thread.
  let measuredPx = 0;
  /// The part that stays when the body is given back: the user's message, its
  /// action row, an error line. It is measured once, at mount, as the article
  /// minus the stand-in it holds then. Taking the whole article as the stand-in
  /// instead would add this height back on every collapse: measured on the
  /// 45-turn harness, one turn went 4356 px rendered → 4454 px collapsed, and
  /// the extra is the header.
  let headerPx = 0;

  $effect(() => {
    if (headerPx > 0 || revealed) return;
    untrack(() => {
      if (article === null || standIn === null) return;
      const height =
        article.getBoundingClientRect().height - standIn.getBoundingClientRect().height;
      if (height > 0) headerPx = height;
    });
  });

  function reveal(): void {
    revealed = true;
    onRendered();
  }

  /// The body is far behind the reader (lib/cull.ts). The newest turn is where
  /// the reader is looking, so it is never given back.
  function collapse(): void {
    if (isLatest || !revealed) return;
    if (measuredPx > 0) placeholder = Math.max(22, Math.round(measuredPx - headerPx));
    revealed = false;
  }

  // What the stand-ins were guessing at, now measured: the transcript averages
  // it into its own figure. Reading the height is untracked so the layout read
  // does not become a dependency of this effect.
  $effect(() => {
    if (!revealed) return;
    untrack(() => {
      const node = article;
      if (node === null) return;
      const height = node.getBoundingClientRect().height;
      if (height > 0) measuredPx = height;
      onMeasured(turn.answer.length, height);
    });
  });
  /// A collapsed `<details>` still puts its text in the DOM, and a long turn
  /// carries hundreds of tool rows. Render a row's payload only once it is
  /// opened.
  let openTools = $state<Set<number>>(new Set());
/// The reasoning fold holds the turn's whole train of thought — tens of KB on a
/// long turn — so it is put in the page when it is opened, like a tool row.
let thinkingOpen = $state(false);

  function setToolOpen(index: number, open: boolean): void {
    const next = new Set(openTools);
    if (open) next.add(index);
    else next.delete(index);
    openTools = next;
  }
  let added = $derived(changes.reduce((total, change) => total + change.added, 0));
  let removed = $derived(
    changes.reduce((total, change) => total + change.removed, 0),
  );
  /// A generated artifact tree can list thousands of paths; render a bounded
  /// sample and keep the true count in the header.
  const SHOWN_CHANGES = 200;
  let shownChanges = $derived(changes.slice(0, SHOWN_CHANGES));
  let hiddenChanges = $derived(Math.max(0, changes.length - SHOWN_CHANGES));

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
      ? Math.min(
          100,
          Math.round((usage.cachedInputTokens / usage.inputTokens) * 100),
        )
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

<article
  class="turn"
  bind:this={article}
  use:cull={{
    root: scrollRoot,
    onReveal: reveal,
    onCollapse: collapse,
    isRevealed: () => revealed,
  }}
>
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

  {#if revealed}
  <!-- Thinking and tool calls share one indented group: both are how the
       turn got to the answer, collapsed until asked for. -->
  {#if turn.thinking || turn.tools.length > 0}
    <div class="tools">
      {#if turn.thinking}
        <details
          class="tool"
          ontoggle={(event) =>
            (thinkingOpen = (event.currentTarget as HTMLDetailsElement).open)}
        >
          <summary>
            <span class="tool-mark">·</span>
            <span class="tool-name">{$t("turn.thinking")}</span>
            <span class="tool-time"
              >{$t("turn.thinkingChars", {
                count: compactCount(turn.thinking.length),
              })}</span
            >
          </summary>
          {#if thinkingOpen}
            <div class="tool-detail">
              <pre>{turn.thinking}</pre>
            </div>
          {/if}
        </details>
      {/if}
      {#each turn.tools as tool, index}
        <details
          class="tool"
          ontoggle={(event) =>
            setToolOpen(
              index,
              (event.currentTarget as HTMLDetailsElement).open,
            )}
        >
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
              {#if tool.reused}
                <span
                  class="tool-reused"
                  title={$t("turn.toolReusedHint", {
                    duration: took(tool.durationMs, tool.ok),
                  })}>{$t("turn.toolReused")}</span
                >
              {:else}
                <span class="tool-time">{took(tool.durationMs, tool.ok)}</span>
              {/if}
            {/if}
          </summary>
          {#if openTools.has(index)}
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
                {#each shownChanges as change}
                  <div class="change-row">
                    <span class="change-status">{change.status}</span>
                    <span class="change-path" title={change.path}>{change.path}</span>
                    <span class="change-add">+{change.added}</span>
                    <span class="change-del">−{change.removed}</span>
                  </div>
                {/each}
                {#if hiddenChanges > 0}
                  <div class="change-row change-more">
                    <span></span>
                    <span class="change-path">
                      {$t("turn.moreChanges", { count: hiddenChanges })}
                    </span>
                  </div>
                {/if}
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
            {#if turn.modelCalls > 0}
              <span title={$t("turn.callsHint")}>
                {$t("turn.calls", { count: turn.modelCalls })}
              </span>
            {/if}
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
  {:else}
    <div
      class="turn-placeholder"
      bind:this={standIn}
      style:height={`${placeholder}px`}
    ></div>
  {/if}

  {#if turn.error}
    <div class="error">{turn.error}</div>
  {/if}
</article>
