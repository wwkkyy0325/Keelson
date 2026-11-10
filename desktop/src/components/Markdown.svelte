<script lang="ts">
  import DOMPurify from "dompurify";
  import { marked } from "marked";

  let { source }: { source: string } = $props();

  // Model output is untrusted text: parse it as markdown, then sanitize the
  // HTML so the transcript cannot inject markup or scripts.
  let html = $derived(
    DOMPurify.sanitize(
      marked.parse(source, { async: false, gfm: true }) as string,
    ),
  );

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
