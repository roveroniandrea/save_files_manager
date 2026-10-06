<script lang="ts">
  import { onMount } from "svelte";
  import { errorDetail, errorMessage, refreshManifest, suggestSavePaths } from "$lib/api";
  import { formatDate } from "$lib/format";
  import { t, tn } from "$lib/i18n";
  import { app } from "$lib/state.svelte";
  import type { SuggestQuery, Suggestions } from "$lib/types";

  let {
    query,
    auto,
    chosen,
    onadd,
  }: {
    /** Read when a search starts (current name/exe of the editor). */
    query: () => SuggestQuery;
    /** Search when shown (Steam games); otherwise on request. */
    auto: boolean;
    /** Folders already in the editor's list. */
    chosen: string[];
    onadd: (path: string) => void;
  } = $props();

  let result = $state<Suggestions | null>(null);
  let loading = $state(false);
  let refreshing = $state(false);

  const isChosen = (path: string) => chosen.some((c) => c.toLowerCase() === path.toLowerCase());

  async function search() {
    loading = true;
    try {
      result = await suggestSavePaths(query());
    } catch (e) {
      app.toast("error", errorMessage(e), errorDetail(e));
    } finally {
      loading = false;
    }
  }

  async function refresh() {
    refreshing = true;
    try {
      const status = await refreshManifest();
      app.toast("info", tn("suggest.refreshed", status.games));
      await search();
    } catch (e) {
      app.toast("error", t("suggest.downloadFailed"), errorDetail(e));
      if (result) result.status.error = errorDetail(e);
    } finally {
      refreshing = false;
    }
  }

  onMount(() => {
    if (auto) search();
  });

  const status = $derived(result?.status);
</script>

<div class="box">
  <div class="head">
    <strong>{t("suggest.title")}</strong>
    <button type="button" class="small" onclick={search} disabled={loading || refreshing}>
      {result ? t("suggest.searchAgain") : t("suggest.find")}
    </button>
  </div>

  {#if loading}
    <p class="muted">{t("suggest.loading")}</p>
  {:else if !result}
    <p class="muted">{t("suggest.hint")}</p>
  {:else if !status?.games}
    <p class="muted">{t("suggest.noDatabase")}</p>
    {#if status?.error}<p class="muted path">{status.error}</p>{/if}
  {:else if !result.matched}
    <p class="muted">{t("suggest.noMatch")}</p>
  {:else}
    <p class="muted">{t("suggest.source", { name: result.matched })}</p>
    {#if result.items.length === 0}
      <p class="muted">{t("suggest.none")}</p>
    {/if}
    <ul>
      {#each result.items as item (item.path)}
        {@const added = isChosen(item.path)}
        <li>
          <div class="info">
            <span class="path" title={item.template}>{item.path}</span>
            <span class="tags">
              {#if item.exists}
                <span class="tag ok">{t("suggest.found")}</span>
              {:else}
                <span class="tag">{t("suggest.missing")}</span>
              {/if}
              {#if item.config && !item.save}<span class="tag">{t("suggest.settings")}</span>{/if}
            </span>
            {#if item.problem}
              <span class="muted problem">{t("suggest.unsafe", { reason: item.problem })}</span>
            {/if}
          </div>
          <button type="button" class="small" disabled={added || !!item.problem} onclick={() => onadd(item.path)}>
            {added ? t("suggest.added") : t("suggest.add")}
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if result}
    <div class="foot muted">
      {#if status?.updated_at}<span>{t("suggest.updated", { date: formatDate(status.updated_at) })}</span>{/if}
      <button type="button" class="link" onclick={refresh} disabled={loading || refreshing}>
        {refreshing ? t("suggest.downloading") : status?.games ? t("suggest.checkUpdates") : t("suggest.download")}
      </button>
    </div>
  {/if}
</div>

<style>
  .box {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .head,
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .foot {
    font-size: 12.5px;
  }
  p {
    margin: 0;
    font-size: 12.5px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  li {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .tags {
    display: flex;
    gap: 4px;
  }
  .tag {
    font-size: 11px;
    padding: 0 6px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--muted);
  }
  .tag.ok {
    color: var(--success);
  }
  .problem {
    font-size: 12px;
  }
  .small {
    padding: 3px 10px;
    font-size: 12.5px;
  }
  .link {
    border: none;
    background: none;
    color: var(--accent);
    padding: 0;
    font-size: 12.5px;
  }
  .link:hover:not(:disabled) {
    background: none;
    text-decoration: underline;
  }
</style>
