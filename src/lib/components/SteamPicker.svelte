<script lang="ts">
  import { onMount } from "svelte";
  import { errorMessage, steamScan } from "$lib/api";
  import { t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";
  import type { ScannedGame, SteamScan } from "$lib/types";
  import Modal from "./Modal.svelte";

  let {
    onpick,
    onmanual,
    onclose,
  }: { onpick: (game: ScannedGame) => void; onmanual: () => void; onclose: () => void } = $props();

  let scan = $state<SteamScan | null>(null);
  let error = $state<string | null>(null);
  let query = $state("");

  onMount(async () => {
    try {
      scan = await steamScan();
    } catch (e) {
      error = errorMessage(e);
    }
  });

  /** App ids already in the list. */
  const added = $derived(
    new Set(app.config?.games.flatMap((g) => (g.source.kind === "steam" ? [g.source.appid] : [])) ?? []),
  );
  const games = $derived(
    (scan?.games ?? []).filter((g) => g.name.toLowerCase().includes(query.trim().toLowerCase())),
  );
</script>

<Modal title={t("steam.pickTitle")} {onclose}>
  {#if error}
    <p class="error">{error}</p>
  {:else if !scan}
    <p class="muted">{t("steam.scanning")}</p>
  {:else if !scan.root}
    <p class="muted">{t("steam.notFound")}</p>
  {:else if scan.games.length === 0}
    <p class="muted">{t("steam.noGames")}</p>
  {:else}
    <!-- svelte-ignore a11y_autofocus -->
    <input type="text" class="search" placeholder={t("steam.search")} bind:value={query} autofocus />
    <ul>
      {#each games as game (game.appid)}
        {@const isAdded = added.has(game.appid)}
        <li>
          <button class="game" disabled={isAdded} onclick={() => onpick(game)}>
            {#if game.icon}
              <img src={game.icon} alt="" width="32" height="32" />
            {:else}
              <span class="placeholder">{game.name.charAt(0).toUpperCase()}</span>
            {/if}
            <span class="name">{game.name}</span>
            {#if isAdded}<span class="muted small">{t("steam.alreadyAdded")}</span>{/if}
          </button>
        </li>
      {:else}
        <li class="muted">{t("steam.noMatch")}</li>
      {/each}
    </ul>
  {/if}

  {#snippet footer()}
    <button type="button" class="other" onclick={onmanual}>{t("steam.otherGame")}</button>
    <button type="button" onclick={onclose}>{t("common.cancel")}</button>
  {/snippet}
</Modal>

<style>
  .search {
    width: 100%;
    margin-bottom: 10px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 55vh;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .game {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    border: none;
    background: none;
    text-align: left;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  img,
  .placeholder {
    width: 32px;
    height: 32px;
    flex-shrink: 0;
    border-radius: 4px;
  }
  .placeholder {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: var(--surface-2);
    color: var(--muted);
    font-weight: 600;
  }
  .small {
    font-size: 12px;
  }
  .other {
    margin-right: auto;
  }
  .error {
    color: var(--danger);
  }
</style>
