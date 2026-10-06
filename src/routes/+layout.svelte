<script lang="ts">
  import "../app.css";

  let { children } = $props();

  // Release builds: no browser context menu (Back, Refresh, …) except in text
  // fields, where copy/paste is useful. Dev builds keep it for "Inspect".
  function oncontextmenu(event: MouseEvent) {
    if (!import.meta.env.PROD) return;
    const target = event.target as HTMLElement | null;
    if (!target?.closest("input, textarea, [contenteditable]")) event.preventDefault();
  }
</script>

<svelte:window {oncontextmenu} />

{@render children()}
