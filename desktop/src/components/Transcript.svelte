<script lang="ts">
  import { onDestroy, tick, untrack } from "svelte";
  import { loadTurns, stopBackground } from "../lib/api";
  import { dispatch, session } from "../lib/session-store";
  import { t } from "../lib/i18n";
  import Turn from "./Turn.svelte";
  import Markdown from "./Markdown.svelte";
  import type { FileChange, Turn as TurnData } from "../lib/protocol";

  let {
    onError,
    error = null,
    projectName,
    changes = [],
    undoable = false,
    onUndo = () => {},
    onBranch = () => {},
  }: {
    onError: (message: string) => void;
    /// A failed action belongs to the conversation, not to a notice bar over
    /// the composer.
    error?: string | null;
    projectName: string;
    /// The latest turn's checkpoint diff; undo belongs to that turn.
    changes?: FileChange[];
    undoable?: boolean;
    onUndo?: () => void;
    onBranch?: (turn: TurnData) => void;
  } = $props();

  let scroller = $state<HTMLDivElement | null>(null);
  let stick = true;
  let suppressAutoScroll = false;

  function mark(ok: boolean | null): string {
    return ok === false ? "✗" : ok === true ? "✓" : "·";
  }

  /// Whole milliseconds; an instant tool reports 0. `<1 ms` is clearer than
  /// an empty column while the call is still running.
  function took(ms: number, ok: boolean | null): string {
    if (ok === null) return "";
    if (ms <= 0) return "<1 ms";
    return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;
  }

  /// The engine measures the command's runtime; the poll refreshes it every
  /// second. The row shows that number directly, so no local clock can drift
  /// from it or starve against the poll.
  function elapsed(ms: number): string {
    const seconds = Math.max(0, Math.floor(ms / 1000));
    if (seconds < 60) return `${seconds} s`;
    const minutes = Math.floor(seconds / 60);
    return `${minutes} m ${String(seconds % 60).padStart(2, "0")} s`;
  }

  /// A long-running command (a dev server, a watcher) has no natural end; the
  /// user can end it here. The engine still hands its result to the model.
  async function stopTask(id: string): Promise<void> {
    try {
      await stopBackground(id);
    } catch (cause) {
      onError(String(cause));
    }
  }

  function compactCount(count: number): string {
    if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
    if (count >= 100_000) return `${Math.round(count / 1000)}k`;
    if (count >= 1000) return `${(count / 1000).toFixed(1)}k`;
    return String(count);
  }

  // Thinking and the commentary from a round that called a tool share one
  // folded block: both are how the turn got to the answer, and the answer
  // area only keeps the call that finished it.
  let reasoningChars = $derived(
    ($session.running?.thinking.length ?? 0) +
      ($session.running?.process.reduce((sum, part) => sum + part.length, 0) ??
        0),
  );

  // Streaming output is sampled, not followed. Each delta rewrites the text of
  // a block that is still growing, and every rewrite costs a layout pass over
  // the transcript, so the stream is read once per window instead. The answer
  // is markdown too: re-parsing it on every chunk flickers.
  const LIVE_FLUSH_MS = 80;
  let answerSource = $state("");
  let thinkingShown = $state("");
  /// The reasoning fold is closed by default and its text is not laid out while
  /// it is, so nothing is copied into it until someone looks.
  let reasoningOpen = $state(false);
  let liveTimer: ReturnType<typeof setTimeout> | null = null;

  function flushLive(): void {
    liveTimer = null;
    const running = $session.running;
    answerSource = running?.text ?? "";
    if (reasoningOpen) thinkingShown = running?.thinking ?? "";
  }

  $effect(() => {
    // Reading the lengths subscribes to the stream; `flushLive` samples the
    // values themselves, so one timer serves both blocks.
    void $session.running?.text.length;
    void $session.running?.thinking.length;
    if (liveTimer !== null) return;
    liveTimer = setTimeout(flushLive, LIVE_FLUSH_MS);
  });

  // A fold opened mid-turn shows what has streamed so far, not the snapshot it
  // was closed with. The read is untracked: once open, the fold is refreshed by
  // the flush above, not once per chunk.
  $effect(() => {
    if (!reasoningOpen) return;
    untrack(() => {
      thinkingShown = $session.running?.thinking ?? "";
    });
  });

  onDestroy(() => {
    if (liveTimer !== null) clearTimeout(liveTimer);
  });

  // The running card shows elapsed time and cost as they accumulate. The
  // effect depends on a derived boolean, not on the store, so streamed deltas
  // do not restart the ticker.
  let runningTurn = $derived($session.running !== null);
  let liveNow = $state(Date.now());
  $effect(() => {
    if (!runningTurn) return;
    liveNow = Date.now();
    const timer = setInterval(() => (liveNow = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  // Keep the last message in view unless the user scrolled up. The effect
  // runs for every delta; one scroll per frame is enough and keeps the
  // scrollbar smooth while the answer grows.
  let followQueued = false;

  function follow(): void {
    if (followQueued) return;
    followQueued = true;
    requestAnimationFrame(() => {
      followQueued = false;
      if (scroller && stick && !suppressAutoScroll) {
        scroller.scrollTop = scroller.scrollHeight;
      }
    });
  }

  $effect(() => {
    const current = $session;
    const failure = error;
    if (suppressAutoScroll || !stick || !scroller) return;
    void current;
    void failure;
    follow();
  });

  let lastTop = 0;

  // A session switch replaces the whole transcript. The browser resets the
  // scroll position, and the scroll handler reads that reset as the user
  // scrolling back, which used to leave an opened conversation at the top.
  // Jump to the newest turn and reset the tracker instead.
  let lastSessionId: string | null = null;
  $effect(() => {
    const id = $session.sessionId;
    if (id === lastSessionId) return;
    lastSessionId = id;
    stick = true;
    requestAnimationFrame(() => {
      if (!scroller) return;
      scroller.scrollTop = scroller.scrollHeight;
      lastTop = scroller.scrollTop;
      stick = true;
    });
  });

  function onScroll(): void {
    if (!scroller) return;
    const top = scroller.scrollTop;
    const atBottom =
      scroller.scrollHeight - top - scroller.clientHeight < 80;
    // A wheel notch is smaller than the 80 px band, so reaching the band is
    // not enough to tell "still following" from "tried to look back". Any
    // upward move releases the follow; moving down follows again at the end.
    stick = top < lastTop ? false : atBottom;
    lastTop = top;
  }

  /// The wheel releases the follow before the browser scrolls, so an
  /// auto-scroll scheduled in the same tick cannot pull the view back.
  function onWheel(event: WheelEvent): void {
    if (event.deltaY < 0) stick = false;
  }

  async function loadOlder(): Promise<void> {
    if (!$session.nextBefore || $session.loadingOlder || !scroller) return;
    const before = $session.nextBefore;
    const previousHeight = scroller.scrollHeight;
    const previousTop = scroller.scrollTop;
    suppressAutoScroll = true;
    dispatch({ type: "loading-older", value: true });
    try {
      const page = await loadTurns(before, 30);
      dispatch({ type: "turns-loaded", page });
      await tick();
      if (scroller) {
        scroller.scrollTop =
          scroller.scrollHeight - previousHeight + previousTop;
      }
    } catch (error) {
      onError(errText(error));
    } finally {
      dispatch({ type: "loading-older", value: false });
      suppressAutoScroll = false;
    }
  }

  function errText(error: unknown): string {
    if (typeof error === "string") return error;
    if (error instanceof Error) return error.message;
    return String(error);
  }
</script>

<div
  class="transcript"
  bind:this={scroller}
  onscroll={onScroll}
  onwheel={onWheel}
>
  {#if $session.nextBefore !== null}
    <button
      class="load-older"
      onclick={loadOlder}
      disabled={$session.loadingOlder}
    >
      {$session.loadingOlder ? $t("app.loading") : $t("app.loadOlder")}
    </button>
  {/if}

  {#if $session.turns.length === 0 && !$session.running}
    <p class="empty">
      {projectName
        ? $t("app.emptyProject", { project: projectName })
        : $t("app.empty")}
    </p>
  {/if}

  {#each $session.turns as turn, index (turn.seq)}
    <Turn
      {turn}
      isLatest={index === $session.turns.length - 1}
      changes={index === $session.turns.length - 1
        ? changes
        : (turn.changes ?? [])}
      {undoable}
      {onUndo}
      onBranch={() => onBranch(turn)}
    />
  {/each}

  {#if $session.running}
    {@const usage = $session.running.usage}
    {@const cacheShare =
      usage.inputTokens > 0
        ? Math.min(
            100,
            Math.round((usage.cachedInputTokens / usage.inputTokens) * 100),
          )
        : 0}
    <article class="turn running">
      <div class="user">{$session.running.input}</div>
      {#if $session.running.thinking || $session.running.tools.length > 0 || $session.running.process.length > 0}
        <div class="tools">
          {#if $session.running.thinking || $session.running.process.length > 0}
            <details class="tool" bind:open={reasoningOpen}>
              <summary>
                <span class="tool-mark">·</span>
                <span class="tool-name">{$t("turn.thinking")}</span>
                <span class="tool-time"
                  >{$t("turn.thinkingChars", {
                    count: compactCount(reasoningChars),
                  })}</span
                >
              </summary>
              <div class="tool-detail">
                {#if thinkingShown}
                  <pre>{thinkingShown}</pre>
                {/if}
                {#each $session.running.process as part}
                  <pre>{part}</pre>
                {/each}
              </div>
            </details>
          {/if}
          {#each $session.running.tools as tool (tool.callId)}
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
      {#if $session.running.text}
        <div class="answer"><Markdown source={answerSource} /></div>
      {/if}
      {#if $session.running.verifying !== null}
        <p class="verifying">
          <span class="verifying-dot"></span>
          {$t("turn.verifying", { count: $session.running.verifying })}
        </p>
      {/if}
      <div class="turn-meta">
        <span title={$t("turn.durationHint")}>
          {elapsed(liveNow - $session.running.startedAt)}
        </span>
        {#if usage.inputTokens > 0 || usage.outputTokens > 0}
          <span title={$t("turn.inputTokensHint")}>
            ↑{compactCount(usage.inputTokens)}
          </span>
          <span title={$t("turn.outputTokensHint")}>
            ↓{compactCount(usage.outputTokens)}
          </span>
        {/if}
        {#if $session.running.modelCalls > 0}
          <span title={$t("turn.callsHint")}>
            {$t("turn.calls", { count: $session.running.modelCalls })}
          </span>
        {/if}
        {#if usage.cachedInputTokens > 0 && usage.inputTokens > 0}
          <span title={$t("turn.cacheHint")}>
            {$t("turn.cacheShare", { percent: cacheShare })}
          </span>
        {/if}
      </div>
    </article>
  {/if}

  <!-- Background commands outlive the turn that started them: they stay at
       the end of the transcript until the engine hands the result to the
       model at the next turn. The row is not a `<details>`: the Stop control
       used to sit inside a `<summary>`, where the webview routes the click to
       the disclosure and the button never fired. -->
  {#if $session.background.length > 0}
    <div class="tools background-strip">
      {#each $session.background as task (task.id)}
        <div class="tool background-row">
          <span
            class:fail={task.ok === false}
            class:ok={task.ok === true}
            class:pending={task.ok === null}
            class="tool-mark">{mark(task.ok)}</span
          >
          <span class="tool-name">{task.id}</span>
          <span class="tool-args" title={task.command || task.program}
            >{task.command || task.program}</span
          >
          {#if task.ok === null}
            <span class="tool-time">{elapsed(task.durationMs)}</span>
            <button
              class="tool-stop"
              onclick={() => void stopTask(task.id)}
              title={$t("turn.backgroundStopHint")}
            >
              {$t("turn.backgroundStop")}
            </button>
          {:else}
            <span class="tool-time">{took(task.durationMs, task.ok)}</span>
          {/if}
        </div>
        {#if task.outputPath}
          <div class="tool-detail">
            <span class="tool-label">{$t("turn.backgroundOutput")}</span>
            <pre>{task.outputPath}</pre>
          </div>
        {/if}
      {/each}
    </div>
  {/if}

  {#if error}
    <p class="turn-error" role="alert">{error}</p>
  {/if}
</div>
