<script lang="ts">
  import { tick } from "svelte";
  import { loadTurns } from "../lib/api";
  import { dispatch, session } from "../lib/session-store";
  import { t } from "../lib/i18n";
  import Turn from "./Turn.svelte";
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

  // Keep the last message in view unless the user scrolled up.
  $effect(() => {
    const current = $session;
    const failure = error;
    if (suppressAutoScroll || !stick || !scroller) return;
    void current;
    void failure;
    tick().then(() => {
      if (scroller && stick && !suppressAutoScroll) {
        scroller.scrollTop = scroller.scrollHeight;
      }
    });
  });

  function onScroll(): void {
    if (!scroller) return;
    stick =
      scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 80;
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

<div class="transcript" bind:this={scroller} onscroll={onScroll}>
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
      changes={index === $session.turns.length - 1 ? changes : []}
      {undoable}
      {onUndo}
      onBranch={() => onBranch(turn)}
    />
  {/each}

  {#if $session.running}
    <article class="turn running">
      <div class="user">{$session.running.input}</div>
      {#if $session.running.thinking || $session.running.tools.length > 0 || $session.running.process.length > 0}
        <div class="tools">
          {#if $session.running.thinking}
            <details class="tool">
              <summary>
                <span class="tool-mark">·</span>
                <span class="tool-name">{$t("turn.thinking")}</span>
              </summary>
              <div class="tool-detail">
                <pre>{$session.running.thinking}</pre>
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
          {#if $session.running.process.length > 0}
            <details class="tool">
              <summary>
                <span class="tool-mark">·</span>
                <span class="tool-name">{$t("turn.process")}</span>
              </summary>
              <div class="tool-detail">
                <pre>{$session.running.process.join("\n\n")}</pre>
              </div>
            </details>
          {/if}
        </div>
      {/if}
      {#if $session.running.text}
        <div class="answer">{$session.running.text}</div>
      {/if}
    </article>
  {/if}

  {#if error}
    <p class="turn-error" role="alert">{error}</p>
  {/if}
</div>
