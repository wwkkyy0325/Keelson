<script lang="ts">
  import { flushSync, onDestroy, tick, untrack } from "svelte";
  import { loadTurns, stopBackground } from "../lib/api";
  import { bottomKey, flushDelay } from "../lib/live";
  import {
    DENSITY_KEY,
    DENSITY_SAMPLES,
    INITIAL_DENSITY,
    learnDensity,
    parseDensity,
  } from "../lib/cull";
  import { AnchorKeeper } from "../lib/anchor";
  import { followAfterScroll } from "../lib/follow";
  import { perf } from "../lib/perf";
  import { visible } from "../lib/visible";
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
    onResume = () => {},
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
    /// Asks the question of an unfinished turn again. Never called on its own:
    /// the turn may have run tools with side effects (ADR 0081).
    onResume?: (input: string) => void;
  } = $props();

  let scroller = $state<HTMLDivElement | null>(null);
  let stick = $state(true);
  let suppressAutoScroll = false;

  /// Pixels of rendered answer per character, so the turns that have not
  /// rendered are reserved at the right height (lib/cull.ts). A figure measured
  /// by an earlier session is used from the first paint: the stand-ins then
  /// match the rendering, and the thread does not grow under the reader as the
  /// bodies fill in.
  let density = $state(storedDensity());
  let measured = $state(0);

  function storedDensity(): number {
    try {
      return parseDensity(localStorage.getItem(DENSITY_KEY)) ?? INITIAL_DENSITY;
    } catch {
      // A webview with storage disabled still renders; it just starts from the
      // default figure.
      return INITIAL_DENSITY;
    }
  }

  function learn(chars: number, renderedPx: number): void {
    if (measured >= DENSITY_SAMPLES || chars < 400 || renderedPx <= 0) return;
    const next = learnDensity(measured, chars, renderedPx, density);
    measured += 1;
    if (next === density) return;
    density = next;
    if (measured >= DENSITY_SAMPLES) rememberDensity(next);
  }

  function rememberDensity(value: number): void {
    try {
      localStorage.setItem(DENSITY_KEY, String(value));
    } catch {
      // The figure is a cache; the next launch measures again.
    }
  }

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
    // The row answers the click at once: the poll that reports the kernel's
    // view runs once a second, and a control that looks dead for that long is
    // read as broken. A failure puts the control back.
    dispatch({ type: "background-stop", id, stopped: true });
    try {
      await stopBackground(id);
    } catch (cause) {
      dispatch({ type: "background-stop", id, stopped: false });
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
  // a block that is still growing, and every rewrite parses the markdown,
  // sanitizes the HTML and lays the block out again, so the stream is read
  // once per window instead. The window grows with the answer: a long answer
  // at a fixed 80 ms spends the core re-rendering itself (lib/live.ts).
  let answerSource = $state("");
  let thinkingShown = $state("");
  /// The reasoning fold is closed by default and its text is not laid out while
  /// it is, so nothing is copied into it until someone looks.
  let reasoningOpen = $state(false);

  /// A collapsed `<details>` is not laid out, but its text is still a node in
  /// the DOM, and a running turn carries a row and a result for every call it
  /// makes — one busy turn put 97 KB of tool output in the page before the
  /// answer arrived. A row's payload is rendered when it is opened instead
  /// (Turn.svelte already does this for a turn that has finished).
  let openTools = $state<Set<number>>(new Set());

  function setToolOpen(callId: number, open: boolean): void {
    const next = new Set(openTools);
    if (open) next.add(callId);
    else next.delete(callId);
    openTools = next;
  }
  let liveTimer: ReturnType<typeof setTimeout> | null = null;

  function flushLive(): void {
    liveTimer = null;
    const running = $session.running;
    // The cost being measured is the render, not the assignment: the markdown
    // behind `answerSource` is parsed during the flush, so the flush is forced
    // here and timed whole.
    const started = performance.now();
    answerSource = running?.text ?? "";
    if (reasoningOpen) thinkingShown = running?.thinking ?? "";
    flushSync();
    perf.mark("live.flush", performance.now() - started);
  }

  $effect(() => {
    // Reading the lengths subscribes to the stream; `flushLive` samples the
    // values themselves, so one timer serves both blocks.
    const chars =
      ($session.running?.text.length ?? 0) +
      (reasoningOpen ? ($session.running?.thinking.length ?? 0) : 0);
    // Nothing is being looked at while the window is hidden or the user has
    // scrolled away from the newest turn: a sample then costs a core and shows
    // nobody. The stream keeps arriving in the store; it is painted again as
    // soon as it is on screen.
    if (!$visible || !stick) {
      if (liveTimer !== null) {
        clearTimeout(liveTimer);
        liveTimer = null;
      }
      return;
    }
    if (liveTimer !== null) return;
    liveTimer = setTimeout(flushLive, flushDelay(chars));
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

  // A row id belongs to one turn; a finished turn's rows live in its own
  // component, so nothing here has to remember them.
  $effect(() => {
    if ($session.running === null && openTools.size > 0) openTools = new Set();
  });

  onDestroy(() => {
    if (liveTimer !== null) clearTimeout(liveTimer);
    keeper.destroy();
  });

  // The running card shows elapsed time and cost as they accumulate. The
  // effect depends on a derived boolean, not on the store, so streamed deltas
  // do not restart the ticker.
  let runningTurn = $derived($session.running !== null);
  let liveNow = $state(Date.now());
  $effect(() => {
    if (!runningTurn || !$visible) return;
    liveNow = Date.now();
    const timer = setInterval(() => (liveNow = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  // Keep the last message in view unless the user scrolled up. One scroll per
  // frame is enough and keeps the scrollbar smooth while the answer grows.
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

  /// What can move the bottom of the transcript, as one figure the effect can
  /// compare (lib/live.ts). The running turn is in it: the live card is
  /// appended the moment a turn starts, and a send that landed below the fold
  /// used to stay there because nothing the follow watched had changed.
  let bottom = $derived(
    bottomKey({
      answerChars: answerSource.length,
      reasoningChars: thinkingShown.length,
      turns: $session.turns.length,
      runningSeq: $session.running?.seq ?? null,
      runningInputChars: $session.running?.input.length ?? 0,
      tools: $session.running?.tools.length ?? 0,
      background: $session.background.length,
      error: error ?? "",
    }),
  );

  $effect(() => {
    void bottom;
    if (suppressAutoScroll || !stick || !scroller) return;
    follow();
  });

  let lastScroll = { top: 0, height: 0 };

  /* ---- the reading position --------------------------------------------
     Every change to the thread's height — a body rendering in place of its
     stand-in, a page of older turns, a narrower window — moves the text under
     the reader unless something puts it back. One keeper does that, for all of
     them (lib/anchor.ts); nothing else writes `scrollTop` except the follow
     scroll and a page load. */
  let anchorEl: HTMLElement | null = null;

  function anchorTop(): number | null {
    if (!scroller || !anchorEl || !anchorEl.isConnected) return null;
    return (
      anchorEl.getBoundingClientRect().top -
      scroller.getBoundingClientRect().top
    );
  }

  /// The turn under the top edge. The thread has gaps between turns, so a few
  /// offsets are tried before giving up for this frame.
  function pickAnchor(): void {
    if (!scroller) return;
    const box = scroller.getBoundingClientRect();
    for (const dy of [0, 8, 24, 48, 90, 160]) {
      const point = document.elementFromPoint(
        box.left + box.width / 2,
        box.top + dy,
      );
      const turn = point instanceof Element ? point.closest(".turn") : null;
      if (turn instanceof HTMLElement && scroller.contains(turn)) {
        anchorEl = turn;
        return;
      }
    }
  }

  const keeper = new AnchorKeeper({
    read: anchorTop,
    shift: (delta) => {
      if (scroller) scroller.scrollTop += delta;
    },
  });

  // A narrower window reflows the markdown: every rendered turn changes height
  // at once, with no reveal involved.
  $effect(() => {
    if (typeof window === "undefined") return;
    const onResize = (): void => keeper.nudge();
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  });

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
    // The figure belongs to the rendering, not to one conversation, so it is
    // carried across a switch; only the sample count restarts.
    measured = 0;
    keeper.own(true);
    requestAnimationFrame(() => {
      if (!scroller) return;
      scroller.scrollTop = scroller.scrollHeight;
      lastScroll = { top: scroller.scrollTop, height: scroller.scrollHeight };
      stick = true;
      keeper.own(false);
    });
  });

  function onScroll(): void {
    if (!scroller) return;
    const top = scroller.scrollTop;
    const height = scroller.scrollHeight;
    // A wheel notch is smaller than the 80 px band, so reaching the band is
    // not enough to tell "still following" from "tried to look back". Any
    // upward move releases the follow; moving down follows again at the end.
    // Content that shrank under the view clamps the position up the same way,
    // and that is not the user looking back (lib/follow.ts).
    stick = followAfterScroll(lastScroll, top, height, scroller.clientHeight);
    lastScroll = { top, height };
    // Where the anchor is now is where the reader put it. A position the
    // follow owns is not the keeper's to defend.
    pickAnchor();
    keeper.own(stick);
    keeper.record();
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
    keeper.own(true);
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
      // The page was spliced in above the reader and the position was already
      // corrected for it; the keeper takes that position as the reader's.
      keeper.own(false);
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
      scrollRoot={scroller}
      density={density}
      onMeasured={learn}
      onRendered={() => keeper.nudge()}
      changes={index === $session.turns.length - 1
        ? changes
        : (turn.changes ?? [])}
      {undoable}
      {onUndo}
      onBranch={() => onBranch(turn)}
      onRetry={() => onResume(turn.input)}
      retryable={index === $session.turns.length - 1 && !$session.running}
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
                {#if reasoningOpen}
                  {#if thinkingShown}
                    <pre>{thinkingShown}</pre>
                  {/if}
                  {#each $session.running.process as part}
                    <pre>{part}</pre>
                  {/each}
                {/if}
              </div>
            </details>
          {/if}
          {#each $session.running.tools as tool (tool.callId)}
            <details
              class="tool"
              ontoggle={(event) =>
                setToolOpen(
                  tool.callId,
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
              {#if openTools.has(tool.callId)}
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

  {#if !$session.running && $session.pending}
    {@const note = $session.pending}
    <!-- A question no turn answers: the app or the turn did not survive it.
         The card keeps what was asked and offers to ask it again; the old turn
         is not replayed, because the tools it already ran had side effects. -->
    <article class="turn interrupted">
      <div class="user">{note.input}</div>
      {#if note.partial}
        <!-- What the answer had already said before the process stopped. Kept
             so a restart does not look as if nothing came back; the card still
             offers to ask again, because a half-run turn had side effects. -->
        <div class="interrupted-answer">{note.partial}</div>
      {/if}
      <div class="interrupted-row">
        <span class="interrupted-note">{$t("turn.interrupted")}</span>
        <button class="interrupted-resume" onclick={() => onResume(note.input)}>
          {$t("turn.resume")}
        </button>
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
            class:fail={task.ok === false && !task.stopped}
            class:ok={task.ok === true}
            class:pending={task.ok === null}
            class:stopped={task.stopped}
            class="tool-mark">{mark(task.ok)}</span
          >
          <span class="tool-name">{task.id}</span>
          <span class="tool-args" title={task.command || task.program}
            >{task.command || task.program}</span
          >
          {#if task.ok === null}
            <span class="tool-time">{elapsed(task.durationMs)}</span>
            {#if task.stopped}
              <!-- The stop was accepted; the process tree ends when the
                   kernel reaps it. Offering the control again would only send
                   a second signal and read as a dead button. -->
              <span class="tool-stopping">{$t("turn.backgroundStopping")}</span>
            {:else}
              <button
                class="tool-stop"
                onclick={() => void stopTask(task.id)}
                title={$t("turn.backgroundStopHint")}
              >
                {$t("turn.backgroundStop")}
              </button>
            {/if}
          {:else}
            <span class="tool-time">{took(task.durationMs, task.ok)}</span>
            {#if task.stopped}
              <span class="tool-stopped">{$t("turn.backgroundStopped")}</span>
            {/if}
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
