<script lang="ts">
  import { onMount } from "svelte";
  import { errorMessage, getConfig } from "$lib/api";
  import type { Config } from "$lib/types";

  let config = $state<Config | null>(null);
  let error = $state<string | null>(null);

  onMount(async () => {
    try {
      config = await getConfig();
    } catch (e) {
      error = errorMessage(e);
    }
  });
</script>

<main class="container">
  <h1>Save Files Manager</h1>
  {#if error}
    <p class="error">{error}</p>
  {:else if config}
    <p>{config.games.length} game(s) configured.</p>
  {:else}
    <p>Loading…</p>
  {/if}
</main>

<style>
  :root {
    font-family: system-ui, "Segoe UI", sans-serif;
    color: #0f0f0f;
    background-color: #f6f6f6;
  }
  .container {
    padding: 1rem 2rem;
  }
  .error {
    color: #b00020;
  }
  @media (prefers-color-scheme: dark) {
    :root {
      color: #f6f6f6;
      background-color: #2f2f2f;
    }
    .error {
      color: #ff7b8a;
    }
  }
</style>
