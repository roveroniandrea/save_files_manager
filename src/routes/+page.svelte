<script lang="ts">
  import { onMount } from "svelte";
  import { checkUpdate, takeRecoveryReports } from "$lib/api";
  import GameDetail from "$lib/components/GameDetail.svelte";
  import GameEditor from "$lib/components/GameEditor.svelte";
  import GameIcon from "$lib/components/GameIcon.svelte";
  import Settings from "$lib/components/Settings.svelte";
  import SteamPicker from "$lib/components/SteamPicker.svelte";
  import Toasts from "$lib/components/Toasts.svelte";
  import UpdateDialog from "$lib/components/UpdateDialog.svelte";
  import { t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";
  import type { ScannedGame } from "$lib/types";

  /** Add-game flow: pick a Steam game (or "other"), then the editor. */
  type Adding = { step: "pick" } | { step: "manual" } | { step: "steam"; game: ScannedGame };
  let adding = $state<Adding | null>(null);

  onMount(async () => {
    await app.load();
    if (app.loadError) return;
    // Startup update check (setting); failures (e.g. offline) are only logged.
    if (app.config?.settings.check_for_updates) {
      checkUpdate()
        .then((update) => {
          if (update) app.update = update;
        })
        .catch((e) => console.warn("update check failed", e));
    }
    for (const report of await takeRecoveryReports().catch(() => [])) {
      const game = report.game_name ?? report.game_id;
      if (report.outcome === "failed") {
        app.toast("error", t("recovery.failed", { game, detail: report.error ?? "" }));
      } else {
        const key = report.outcome === "completed" ? "recovery.completed" : "recovery.restored";
        app.toast("info", t(key, { game }), undefined, true);
      }
    }
  });

  const games = $derived([...(app.config?.games ?? [])].sort((a, b) => a.name.localeCompare(b.name)));
</script>

{#if app.loadError}
  <main class="fatal">
    <h1>{t("app.title")}</h1>
    <p class="error">{app.loadError.summary}</p>
    <p class="path muted">{app.loadError.detail}</p>
  </main>
{:else if !app.config}
  <main class="fatal"><p class="muted">{t("app.loading")}</p></main>
{:else}
  <div class="layout">
    <nav>
      <div class="brand">{t("app.title")}</div>
      <button class="primary add" onclick={() => (adding = { step: "pick" })} disabled={app.busy}>+ {t("library.add")}</button>
      <ul>
        {#each games as game (game.id)}
          {@const active = game.profiles.find((p) => p.id === game.active_profile_id)}
          <li>
            <button
              class="game"
              class:selected={app.view === "game" && game.id === app.selectedId}
              onclick={() => {
                app.selectedId = game.id;
                app.view = "game";
              }}
            >
              <GameIcon {game} size={28} />
              <span class="label">
                <span class="name">{game.name}</span>
                {#if active}<span class="profile muted">{active.name}</span>{/if}
              </span>
            </button>
          </li>
        {/each}
      </ul>
      <button class="settings-link" class:selected={app.view === "settings"} onclick={() => (app.view = "settings")}>
        ⚙ {t("settings.open")}
      </button>
    </nav>
    <main>
      {#if app.view === "settings"}
        <Settings />
      {:else if app.selected}
        {#key app.selected.id}
          <GameDetail game={app.selected} />
        {/key}
      {:else}
        <p class="empty muted">{games.length ? t("library.noSelection") : t("library.empty")}</p>
      {/if}
    </main>
  </div>
{/if}

{#if adding?.step === "pick"}
  <SteamPicker
    onpick={(game) => (adding = { step: "steam", game })}
    onmanual={() => (adding = { step: "manual" })}
    onclose={() => (adding = null)}
  />
{:else if adding?.step === "manual"}
  <GameEditor onclose={() => (adding = null)} />
{:else if adding?.step === "steam"}
  <GameEditor steam={adding.game} onclose={() => (adding = null)} />
{/if}

{#if app.update}
  <UpdateDialog update={app.update} />
{/if}

<Toasts />

<style>
  .layout {
    display: grid;
    grid-template-columns: 260px 1fr;
    height: 100vh;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 10px;
    border-right: 1px solid var(--border);
    background: var(--surface);
    overflow-y: auto;
  }
  .brand {
    font-weight: 600;
    padding: 0 6px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .game {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 8px;
    border: none;
    background: none;
    text-align: left;
  }
  .settings-link {
    margin-top: auto;
    border: none;
    background: none;
    text-align: left;
  }
  .settings-link.selected,
  .game.selected {
    background: var(--surface-2);
  }
  .label {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name,
  .profile {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .profile {
    font-size: 12px;
  }
  main {
    padding: 22px 28px;
    overflow-y: auto;
  }
  .empty {
    margin-top: 40px;
    text-align: center;
  }
  .fatal {
    padding: 32px;
  }
  .error {
    color: var(--danger);
  }
</style>
