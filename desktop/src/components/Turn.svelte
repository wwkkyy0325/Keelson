<script lang="ts">
  import { t } from "../lib/i18n";
  import type { Turn } from "../lib/protocol";

  let { turn }: { turn: Turn } = $props();
</script>

<article class="turn">
  <div class="user">{turn.input}</div>
  {#if turn.answer}
    <div class="answer">{turn.answer}</div>
  {/if}
  {#if turn.state !== "Done" && !turn.answer}
    <div class="state">{turn.state}</div>
  {/if}
  {#if turn.error}
    <div class="error">{turn.error}</div>
  {/if}
  {#if turn.verificationPassed !== null}
    <div class="verification" class:passed={turn.verificationPassed}>
      <span>{turn.verificationPassed ? "✓" : "✗"}</span>
      <span>
        {turn.verificationPassed
          ? $t("app.verified")
          : $t("app.verificationFailed")}
      </span>
    </div>
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
