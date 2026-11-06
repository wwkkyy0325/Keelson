<script lang="ts">
  import { tick } from "svelte";
  import { loadTurns } from "../lib/api";
  import { dispatch, session } from "../lib/session-store";
  import { t } from "../lib/i18n";
  import Turn from "./Turn.svelte";

  let { onError }: { onError: (message: string) => void } = $props();

  let scroller = $state<HTMLDivElement | null>(null);
  let stick = true;
  let suppressAutoScroll = false;

  // Keep the last message in view unless the user scrolled up.
  $effect(() => {
    const current = $session;
    if (suppressAutoScroll || !stick || !scroller) return;
    void current;
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
    <p class="empty">{$t("app.empty")}</p>
  {/if}

  {#each $session.turns as turn (turn.seq)}
    <Turn {turn} />
  {/each}

  {#if $session.running}
    <article class="turn running">
      <div class="user">{$session.running.input}</div>
      <div class="answer">
        {$session.running.text || $t("app.working")}<span class="cursor">▍</span>
      </div>
    </article>
  {/if}
</div>
