<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { open } from "@tauri-apps/plugin-dialog";
  import {
    errorDetail,
    errorMessage,
    manifestStatus,
    openStorageFolder,
    refreshManifest,
    updateSettings,
  } from "$lib/api";
  import { formatDate } from "$lib/format";
  import { t, tn } from "$lib/i18n";
  import { app } from "$lib/state.svelte";
  import type { ManifestStatus, Settings, Theme } from "$lib/types";

  let manifest = $state<ManifestStatus | null>(null);
  let checking = $state(false);
  let version = $state("");

  const settings = $derived(app.config!.settings);
  const hasGames = $derived((app.config?.games.length ?? 0) > 0);

  onMount(async () => {
    manifest = await manifestStatus().catch(() => null);
    version = await getVersion().catch(() => "");
  });

  function save(patch: Partial<Settings>) {
    return app.run(() => updateSettings({ ...$state.snapshot(settings), ...patch }));
  }

  async function checkManifest() {
    checking = true;
    try {
      manifest = await refreshManifest();
      app.toast("info", tn("suggest.refreshed", manifest.games));
    } catch (e) {
      app.toast("error", t("suggest.downloadFailed"), errorDetail(e));
      manifest = await manifestStatus().catch(() => manifest);
    } finally {
      checking = false;
    }
  }

  async function changeStorage() {
    const path = await open({ title: t("settings.pickStorage"), directory: true });
    if (path) await save({ storage_root: path });
  }

  async function openStorage() {
    try {
      await openStorageFolder();
    } catch (e) {
      app.toast("error", errorMessage(e), errorDetail(e));
    }
  }

  const themes: Theme[] = ["system", "light", "dark"];
  const limits = [1, 2, 3, 5, 10, 20, 50];
</script>

<section class="settings">
  <h1>{t("settings.title")}</h1>

  <h2>{t("settings.appearance")}</h2>
  <label>
    {t("settings.theme")}
    <select value={settings.theme} onchange={(e) => save({ theme: e.currentTarget.value as Theme })} disabled={app.busy}>
      {#each themes as theme (theme)}
        <option value={theme}>{t(`settings.theme.${theme}`)}</option>
      {/each}
    </select>
  </label>

  <h2>{t("settings.backups")}</h2>
  <label>
    {t("settings.backupLimit")}
    <select
      value={settings.backup_limit}
      onchange={(e) => save({ backup_limit: Number(e.currentTarget.value) })}
      disabled={app.busy}
    >
      {#each limits as n (n)}
        <option value={n}>{n}</option>
      {/each}
    </select>
    <span class="muted hint">{t("settings.backupLimitHint")}</span>
  </label>

  <h2>{t("settings.saveLocations")}</h2>
  <label class="check">
    <input
      type="checkbox"
      checked={settings.include_config}
      onchange={(e) => save({ include_config: e.currentTarget.checked })}
      disabled={app.busy}
    />
    {t("settings.includeConfig")}
  </label>
  <div class="row">
    <div class="grow">
      <div>{t("settings.database")}</div>
      <div class="muted hint">
        {#if manifest?.games}
          {tn("settings.databaseGames", manifest.games)}
          {#if manifest.updated_at}· {t("suggest.updated", { date: formatDate(manifest.updated_at) })}{/if}
          {#if manifest.checked_at}· {t("settings.databaseChecked", { date: formatDate(manifest.checked_at) })}{/if}
        {:else}
          {t("suggest.noDatabase")}
        {/if}
      </div>
      {#if manifest?.error}<div class="muted path">{manifest.error}</div>{/if}
    </div>
    <button onclick={checkManifest} disabled={checking}>
      {checking ? t("suggest.downloading") : t("suggest.checkUpdates")}
    </button>
  </div>

  <h2>{t("settings.storage")}</h2>
  <div class="row">
    <div class="grow">
      <div class="path">{settings.storage_root ?? t("settings.storageDefault")}</div>
      <div class="muted hint">{hasGames ? t("settings.storageLocked") : t("settings.storageHint")}</div>
    </div>
    <button onclick={openStorage}>{t("settings.openFolder")}</button>
    {#if !hasGames}
      <button onclick={changeStorage} disabled={app.busy}>{t("settings.change")}</button>
      {#if settings.storage_root}
        <button onclick={() => save({ storage_root: null })} disabled={app.busy}>{t("settings.reset")}</button>
      {/if}
    {/if}
  </div>

  {#if version}
    <p class="muted version">{t("settings.version", { version })}</p>
  {/if}
</section>

<style>
  .settings {
    max-width: 720px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  h1 {
    margin: 0;
    font-size: 22px;
  }
  h2 {
    font-size: 15px;
    margin: 18px 0 0;
  }
  label > select {
    display: block;
    margin-top: 4px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .hint {
    display: block;
    font-size: 12.5px;
    margin-top: 2px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .version {
    margin-top: 24px;
    font-size: 12.5px;
  }
</style>
