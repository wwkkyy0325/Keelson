<script lang="ts">
  import type { Interaction } from "../lib/protocol";
  import { permissionLabel, t } from "../lib/i18n";

  let {
    interaction,
    answering,
    onAnswer,
    onDismiss,
  }: {
    interaction: Interaction;
    answering: string | null;
    onAnswer: (option: string) => void;
    onDismiss: () => void;
  } = $props();
</script>

<div class="permission">
  <div class="permission-kind">
    {interaction.kind === "permission_approval"
      ? $t("app.permission")
      : interaction.kind === "clarify"
        ? $t("app.clarify")
        : interaction.kind === "choice"
          ? $t("app.choice")
          : interaction.kind}
  </div>
  <div class="permission-prompt">{interaction.prompt}</div>
  <div class="permission-options">
    {#each interaction.options as option (option.id)}
      <button
        class:recommended={interaction.recommended === option.id}
        onclick={() => onAnswer(option.id)}
        disabled={answering !== null}
      >
        {$permissionLabel(option.id, option.label)}
        {#if interaction.recommended === option.id}
          <span class="badge">{$t("app.recommended")}</span>
        {/if}
      </button>
    {/each}
  </div>
  {#if interaction.kind !== "permission_approval"}
    <div class="permission-dismiss">
      <button
        class="ghost"
        title={$t("interaction.dismissHint")}
        onclick={onDismiss}
        disabled={answering !== null}
      >
        {$t("interaction.dismiss")}
      </button>
    </div>
  {/if}
</div>
