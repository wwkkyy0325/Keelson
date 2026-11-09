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

  function copyTip(key: string): string {
    return copied === key ? $t("turn.copied") : $t("turn.copy");
  }
</script>

<article class="turn">
  <div class="turn-block">
    <div class="user">{turn.input}</div>
    <div class="turn-actions user-actions">
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

  {#if turn.answer}
    <div class="turn-block">
      <div class="answer">{turn.answer}</div>
      <div class="turn-actions">
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
