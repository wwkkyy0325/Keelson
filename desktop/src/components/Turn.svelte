<script lang="ts">
  import { t } from "../lib/i18n";
  import type { Turn } from "../lib/protocol";

  let {
    turn,
    canUndo = false,
    onUndo = () => {},
    onBranch = () => {},
  }: {
    turn: Turn;
    canUndo?: boolean;
    onUndo?: () => void;
    onBranch?: () => void;
  } = $props();

  let passed = $derived(turn.verification?.passed ?? turn.verificationPassed);
  let copied = $state<string | null>(null);

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

  function copyLabel(key: string): string {
    return copied === key ? $t("turn.copied") : $t("turn.copy");
  }
</script>

<article class="turn">
  <div class="turn-line user-line">
    <div class="turn-actions">
      <button class="link turn-action" onclick={() => copy(turn.input, "input")}>
        {copyLabel("input")}
      </button>
    </div>
    <div class="user">{turn.input}</div>
  </div>
  {#if turn.answer}
    <div class="turn-line answer-line">
      <div class="answer">{turn.answer}</div>
      <div class="turn-actions">
        <button
          class="link turn-action"
          onclick={() => copy(turn.answer, "answer")}
        >
          {copyLabel("answer")}
        </button>
        <button class="link turn-action" onclick={onBranch}>
          {$t("turn.branch")}
        </button>
      </div>
    </div>
  {/if}
  {#if turn.state !== "Done" && !turn.answer}
    <div class="state">{turn.state}</div>
  {/if}
  {#if turn.error}
    <div class="error">{turn.error}</div>
  {/if}
  {#if passed !== null || canUndo}
    <div class="turn-foot">
      {#if passed !== null}
        <div class="verification" class:passed={passed}>
          <span>{passed ? "✓" : "✗"}</span>
          <span>
            {passed ? $t("app.verified") : $t("app.verificationFailed")}
          </span>
        </div>
      {/if}
      {#if canUndo}
        <button class="link turn-undo" onclick={onUndo}>
          {$t("app.undoTurn")}
        </button>
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
  {#if turn.tools.length > 0}
    <div class="tools">
      {#each turn.tools as tool}
        <details class="tool">
          <summary>
            <span class:fail={tool.ok === false} class:ok={tool.ok === true}>
              {tool.ok === false ? "✗" : "✓"}
            </span>
            <span class="tool-name">{tool.name}</span>
            {#if tool.durationMs > 0}
              <span class="dim">{tool.durationMs} ms</span>
            {/if}
          </summary>
          {#if tool.args}<pre>{tool.args}</pre>{/if}
          {#if tool.output}<pre>{tool.output}</pre>{/if}
        </details>
      {/each}
    </div>
  {/if}
</article>
