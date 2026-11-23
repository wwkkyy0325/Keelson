<script lang="ts">
  import { renderMarkdown } from "../lib/markdown";

  let { source }: { source: string } = $props();

  // The HTML comes from a memoized render (lib/markdown.ts). The transcript
  // re-renders on every streamed delta and every poll tick; without the memo
  // each of those parsed and sanitized this answer again, which held a core at
  // 100% on a thread of long answers while nothing on screen was moving. A text
  // that has not changed is a map lookup now.
  let html = $derived(renderMarkdown(source));

  // A link inside the transcript must not navigate the webview away from the
  // conversation. The text stays selectable and copyable.
  function blockLinks(node: HTMLElement): { destroy: () => void } {
    const handler = (event: MouseEvent): void => {
      if ((event.target as HTMLElement).closest("a")) {
        event.preventDefault();
      }
    };
    node.addEventListener("click", handler);
    return { destroy: () => node.removeEventListener("click", handler) };
  }
</script>

<div class="markdown" use:blockLinks>{@html html}</div>
