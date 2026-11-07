<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "../lib/i18n";

  let {
    title,
    message,
    confirmLabel,
    onConfirm,
    onCancel,
  }: {
    title: string;
    message: string;
    confirmLabel: string;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  let cancelButton = $state<HTMLButtonElement | null>(null);

  onMount(() => cancelButton?.focus());

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      onCancel();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<button
  class="dialog-backdrop"
  aria-label={$t("app.cancel")}
  onclick={onCancel}
></button>
<div
  class="dialog"
  role="dialog"
  aria-modal="true"
  aria-labelledby="confirm-dialog-title"
>
  <h2 id="confirm-dialog-title">{title}</h2>
  <p>{message}</p>
  <div class="dialog-actions">
    <button bind:this={cancelButton} onclick={onCancel}>
      {$t("app.cancel")}
    </button>
    <button class="danger" onclick={onConfirm}>{confirmLabel}</button>
  </div>
</div>
