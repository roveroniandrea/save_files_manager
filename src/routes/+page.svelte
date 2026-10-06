<script lang="ts">
  import { onMount } from "svelte";
  import { errorDetail, errorMessage, getConfig, takeRecoveryReports } from "$lib/api";
  import { t, tn } from "$lib/i18n";
  import type { Config, RecoveryReport } from "$lib/types";

  let config = $state<Config | null>(null);
  let error = $state<{ summary: string; detail: string } | null>(null);
  let recovery = $state<RecoveryReport[]>([]);

  onMount(async () => {
    try {
      config = await getConfig();
      recovery = await takeRecoveryReports();
    } catch (e) {
      error = { summary: errorMessage(e), detail: errorDetail(e) };
    }
  });
</script>

<main class="container">
  <h1>{t("app.title")}</h1>
  {#each recovery as report (report.game_id)}
    <p class={report.outcome === "failed" ? "error" : "notice"}>
      {#if report.outcome === "failed"}
        {t("recovery.failed", { game: report.game_name ?? report.game_id, detail: report.error ?? "" })}
      {:else if report.outcome === "completed"}
        {t("recovery.completed", { game: report.game_name ?? report.game_id })}
      {:else}
        {t("recovery.restored", { game: report.game_name ?? report.game_id })}
      {/if}
    </p>
  {/each}
  {#if error}
    <p class="error" title={error.detail}>{error.summary}</p>
  {:else if config}
    <p>{tn("library.gameCount", config.games.length)}</p>
  {:else}
    <p>{t("app.loading")}</p>
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
  .notice {
    color: #6a4a00;
  }
  @media (prefers-color-scheme: dark) {
    :root {
      color: #f6f6f6;
      background-color: #2f2f2f;
    }
    .error {
      color: #ff7b8a;
    }
    .notice {
      color: #f0c674;
    }
  }
</style>
