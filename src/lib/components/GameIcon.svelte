<script lang="ts">
  import { getIcon } from "$lib/api";
  import type { Game } from "$lib/types";

  let { game, size = 32 }: { game: Game; size?: number } = $props();

  let src = $state<string | null>(null);

  // Re-fetched whenever the game object changes (each config update).
  $effect(() => {
    const { id, icon } = game;
    let cancelled = false;
    if (!icon) {
      src = null;
      return;
    }
    getIcon(id)
      .then((url) => {
        if (!cancelled) src = url;
      })
      .catch(() => {
        if (!cancelled) src = null;
      });
    return () => {
      cancelled = true;
    };
  });
</script>

{#if src}
  <img {src} alt="" width={size} height={size} />
{:else}
  <span class="placeholder" style:width="{size}px" style:height="{size}px" style:font-size="{size * 0.5}px">
    {game.name.charAt(0).toUpperCase()}
  </span>
{/if}

<style>
  img {
    object-fit: contain;
    flex-shrink: 0;
  }
  .placeholder {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--muted);
    font-weight: 600;
  }
</style>
