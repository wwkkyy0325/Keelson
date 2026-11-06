<script lang="ts">
  import type { Interaction } from "../lib/protocol";
  import { permissionLabel, t } from "../lib/i18n";

  let {
    interaction,
    answering,
    onAnswer,
  }: {
    interaction: Interaction;
    answering: string | null;
    onAnswer: (option: string) => void;
  } = $props();
</script>

<div class="permission">
  <div class="permission-kind">
    {interaction.kind === "permission_approval"
      ? $t("app.permission")
      : interaction.kind}
  </div>
  <div class="permission-prompt">{interaction.prompt}</div>
  <div class="permission-options">
    {#each interaction.options as option (option.id)}
      <button onclick={() => onAnswer(option.id)} disabled={answering !== null}>
        {$permissionLabel(option.id, option.label)}
      </button>
    {/each}
  </div>
</div>
