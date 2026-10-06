<script lang="ts">
  import { ask } from "@tauri-apps/plugin-dialog";
  import {
    createProfile,
    deleteProfile,
    errorDetail,
    errorMessage,
    gameRunning,
    isAppError,
    launchGame,
    listBackups,
    profileStats,
    renameProfile,
    restoreBackup,
    steamCloudStatus,
    switchProfile,
  } from "$lib/api";
  import { formatBytes, formatDate } from "$lib/format";
  import { t, tn } from "$lib/i18n";
  import { app } from "$lib/state.svelte";
  import type { BackupInfo, CloudStatus, Game, Profile, ProfileStats, Uuid } from "$lib/types";
  import GameEditor from "./GameEditor.svelte";
  import GameIcon from "./GameIcon.svelte";
  import RemoveGameDialog from "./RemoveGameDialog.svelte";

  let { game }: { game: Game } = $props();

  let stats = $state<Record<Uuid, ProfileStats>>({});
  let editing = $state(false);
  let removing = $state(false);
  let renamingId = $state<Uuid | null>(null);
  let renameValue = $state("");
  let creating = $state(false);
  let newName = $state("");
  /** "" = fresh, otherwise the id of the profile to clone. */
  let newSource = $state("");

  // Sizes/dates are refreshed after every config change (switch, create, …).
  $effect(() => {
    const id = game.id;
    let cancelled = false;
    profileStats(id)
      .then((list) => {
        if (!cancelled) stats = Object.fromEntries(list.map((s) => [s.profile_id, s]));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  // Running state, polled while the page is shown: switches are refused while the game runs.
  let running = $state(false);
  $effect(() => {
    const id = game.id;
    let cancelled = false;
    const poll = () =>
      gameRunning(id)
        .then((r) => {
          if (!cancelled) running = r;
        })
        .catch(() => {});
    poll();
    const timer = setInterval(poll, 3000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  });

  // Backups, reloaded after every config change (switches and restores add some).
  let backups = $state<BackupInfo[]>([]);
  $effect(() => {
    const id = game.id;
    void game; // new object after every config change
    let cancelled = false;
    listBackups(id)
      .then((list) => {
        if (!cancelled) backups = list;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  /** Profile to play as; follows the active profile until changed. */
  let playAs = $state<Uuid | null>(null);
  $effect(() => {
    playAs = game.active_profile_id;
  });
  const canPlay = $derived(game.source.kind === "steam" || !!game.exe_path);

  // Steam Cloud warning (Steam games only); checked when the game is shown.
  let cloud = $state<CloudStatus | null>(null);
  $effect(() => {
    if (game.source.kind !== "steam") return;
    let cancelled = false;
    steamCloudStatus(game.id)
      .then((status) => {
        if (!cancelled) cloud = status;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  const cloudAccounts = $derived(
    cloud?.accounts.map((a) => a.persona_name ?? String(a.account_id)).join(", ") ?? "",
  );

  const profiles = $derived(
    [...game.profiles].sort((a, b) =>
      a.id === game.active_profile_id ? -1 : b.id === game.active_profile_id ? 1 : a.name.localeCompare(b.name),
    ),
  );

  /** Switches profile (asking when live saves are empty); true on success. */
  async function switchTo(profile: Profile, allowEmptyLive = false): Promise<boolean> {
    let needsConfirm = false;
    const ok = await app.run(
      () => switchProfile(game.id, profile.id, { allow_empty_live: allowEmptyLive }),
      (e) => (needsConfirm = isAppError(e) && e.kind === "live_saves_empty"),
    );
    if (ok) {
      app.toast("info", t("switch.done", { game: game.name, profile: profile.name }));
      return true;
    }
    if (needsConfirm) {
      const active = game.profiles.find((p) => p.id === game.active_profile_id);
      const yes = await ask(t("switch.emptyConfirm", { game: game.name, profile: active?.name ?? "" }), {
        title: t("switch.emptyTitle"),
        kind: "warning",
        okLabel: t("switch.continue"),
        cancelLabel: t("common.cancel"),
      });
      if (yes) return switchTo(profile, true);
    }
    return false;
  }

  /** Switches to the "play as" profile if needed, then starts the game. */
  async function play() {
    const target = game.profiles.find((p) => p.id === playAs);
    if (target && target.id !== game.active_profile_id && !(await switchTo(target))) return;
    app.busy = true;
    try {
      await launchGame(game.id);
      app.toast("info", t("play.started", { game: game.name }));
      setTimeout(() => gameRunning(game.id).then((r) => (running = r)).catch(() => {}), 1500);
    } catch (e) {
      app.toast("error", errorMessage(e), errorDetail(e));
    } finally {
      app.busy = false;
    }
  }

  async function restore(backup: BackupInfo) {
    const profile = game.profiles.find((p) => p.id === backup.profile_id);
    const yes = await ask(
      t("backups.restoreConfirm", { profile: profile?.name ?? "", date: formatDate(backup.created_at) }),
      {
        title: t("backups.restoreTitle"),
        kind: "warning",
        okLabel: t("backups.restore"),
        cancelLabel: t("common.cancel"),
      },
    );
    if (yes && (await app.run(() => restoreBackup(game.id, backup.id)))) {
      app.toast("info", t("backups.restored"));
    }
  }

  function startRename(profile: Profile) {
    renamingId = profile.id;
    renameValue = profile.name;
  }

  async function submitRename(event: Event) {
    event.preventDefault();
    const id = renamingId;
    if (id && (await app.run(() => renameProfile(game.id, id, renameValue)))) renamingId = null;
  }

  async function remove(profile: Profile) {
    const yes = await ask(t("profile.deleteConfirm", { name: profile.name }), {
      title: t("profile.deleteTitle"),
      kind: "warning",
      okLabel: t("profile.delete"),
      cancelLabel: t("common.cancel"),
    });
    if (yes) await app.run(() => deleteProfile(game.id, profile.id));
  }

  async function submitNew(event: Event) {
    event.preventDefault();
    const source = newSource ? { kind: "clone" as const, profile_id: newSource } : { kind: "fresh" as const };
    if (await app.run(() => createProfile(game.id, newName, source))) {
      creating = false;
      newName = "";
      newSource = "";
    }
  }
</script>

<section class="detail">
  <header>
    <GameIcon {game} size={48} />
    <div class="title">
      <h1>{game.name}</h1>
      <span class="muted">
        {tn("game.profileCount", game.profiles.length)}
        {#if game.source.kind === "steam"}· {t("game.steamAppId", { appid: game.source.appid })}{/if}
      </span>
    </div>
    <div class="play">
      {#if game.profiles.length > 1}
        <select bind:value={playAs} disabled={app.busy || running} aria-label={t("play.as")} title={t("play.as")}>
          {#each profiles as p (p.id)}
            <option value={p.id}>{p.name}</option>
          {/each}
        </select>
      {/if}
      <button
        class="primary"
        onclick={play}
        disabled={app.busy || running || !canPlay}
        title={canPlay ? "" : t("play.noExecutable")}
      >
        ▶ {t("play.play")}
      </button>
    </div>
    <button onclick={() => (editing = true)} disabled={app.busy}>{t("game.edit")}</button>
    <button class="danger" onclick={() => (removing = true)} disabled={app.busy}>{t("game.remove")}</button>
  </header>

  {#if running}
    <div class="warning running" role="status">
      <strong>{t("play.running")}</strong>
      <p>{t("play.runningBody")}</p>
    </div>
  {/if}

  {#if cloud && cloud.accounts.length > 0}
    <div class="warning" role="alert">
      <strong>{t("cloud.title")}</strong>
      <p>{t("cloud.body", { accounts: cloudAccounts })}</p>
      <p>{t("cloud.howTo")}</p>
    </div>
  {/if}

  <div class="section-head">
    <h2>{t("game.profiles")}</h2>
    {#if !creating}
      <button onclick={() => (creating = true)} disabled={app.busy}>{t("profile.new")}</button>
    {/if}
  </div>

  {#if creating}
    <form class="card new" onsubmit={submitNew}>
      <label>
        {t("profile.name")}
        <!-- svelte-ignore a11y_autofocus -->
        <input type="text" bind:value={newName} required autofocus />
      </label>
      <label>
        {t("profile.startFrom")}
        <select bind:value={newSource}>
          <option value="">{t("profile.fresh")}</option>
          {#each profiles as p (p.id)}
            <option value={p.id}>{t("profile.cloneOf", { name: p.name })}</option>
          {/each}
        </select>
      </label>
      <div class="actions">
        <button type="button" onclick={() => (creating = false)}>{t("common.cancel")}</button>
        <button type="submit" class="primary" disabled={app.busy || !newName.trim()}>{t("profile.create")}</button>
      </div>
    </form>
  {/if}

  <ul class="profiles">
    {#each profiles as profile (profile.id)}
      {@const active = profile.id === game.active_profile_id}
      {@const s = stats[profile.id]}
      <li class="card" class:active>
        <div class="main">
          {#if renamingId === profile.id}
            <form class="rename" onsubmit={submitRename}>
              <!-- svelte-ignore a11y_autofocus -->
              <input type="text" bind:value={renameValue} required autofocus />
              <button type="submit" class="primary" disabled={app.busy}>{t("common.save")}</button>
              <button type="button" onclick={() => (renamingId = null)}>{t("common.cancel")}</button>
            </form>
          {:else}
            <div class="name">
              <strong>{profile.name}</strong>
              {#if active}<span class="badge">{t("profile.active")}</span>{/if}
            </div>
          {/if}
          <div class="meta muted">
            <span>
              {profile.last_used_at
                ? t("profile.lastUsed", { date: formatDate(profile.last_used_at) })
                : t("profile.neverUsed")}
            </span>
            {#if s}
              <span>·</span>
              {#if s.last_modified}
                <span>{formatBytes(s.size_bytes)}</span>
                <span>·</span>
                <span>{t("profile.modified", { date: formatDate(s.last_modified) })}</span>
              {:else}
                <span>{t("profile.noSaves")}</span>
              {/if}
            {/if}
          </div>
        </div>
        <div class="actions">
          {#if !active}
            <button class="primary" disabled={app.busy || running} onclick={() => switchTo(profile)}>
              {t("profile.switch")}
            </button>
          {/if}
          <button disabled={app.busy} onclick={() => startRename(profile)}>{t("profile.rename")}</button>
          {#if !active}
            <button class="danger" disabled={app.busy} onclick={() => remove(profile)}>{t("profile.delete")}</button>
          {/if}
        </div>
      </li>
    {/each}
  </ul>

  <h2>{t("game.saveFolders")}</h2>
  <ul class="folders">
    {#each game.save_paths as path (path.id)}
      <li class="path" title={path.template}>{path.resolved}</li>
    {/each}
  </ul>

  <h2>{t("backups.title")}</h2>
  {#if backups.length === 0}
    <p class="muted">{t("backups.none")}</p>
  {:else}
    <p class="muted hint">{t("backups.hint")}</p>
    <ul class="backups">
      {#each backups as backup (backup.id)}
        {@const profile = game.profiles.find((p) => p.id === backup.profile_id)}
        {@const isActive = backup.profile_id === game.active_profile_id}
        <li>
          <div class="main">
            <div>{formatDate(backup.created_at)}</div>
            <div class="muted meta">
              {t(backup.reason === "restore" ? "backups.reasonRestore" : "backups.reasonSwitch", {
                profile: profile?.name ?? t("backups.deletedProfile"),
              })}
            </div>
          </div>
          <button
            disabled={app.busy || running || !isActive}
            title={isActive ? "" : t("backups.onlyActive")}
            onclick={() => restore(backup)}
          >
            {t("backups.restore")}
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  <h2>{t("game.executable")}</h2>
  <p class="path">{game.exe_path ?? t("game.noExecutable")}</p>

  {#if game.source.kind === "steam"}
    <h2>{t("game.installFolder")}</h2>
    <p class="path">{game.source.install_dir}</p>
  {/if}
</section>

{#if editing}
  <GameEditor {game} onclose={() => (editing = false)} />
{/if}
{#if removing}
  <RemoveGameDialog {game} onclose={() => (removing = false)} />
{/if}

<style>
  .detail {
    max-width: 860px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 20px;
  }
  .title {
    flex: 1;
    min-width: 0;
  }
  h1 {
    margin: 0;
    font-size: 22px;
    overflow-wrap: anywhere;
  }
  h2 {
    font-size: 15px;
    margin: 24px 0 10px;
  }
  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .profiles {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .card {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }
  .card.active {
    border-color: var(--accent);
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .card.new {
    flex-wrap: wrap;
    align-items: flex-end;
    margin-bottom: 8px;
  }
  .card.new label {
    flex: 1 1 200px;
  }
  .card.new input,
  .card.new select {
    display: block;
    width: 100%;
    margin-top: 4px;
  }
  .main {
    flex: 1;
    min-width: 0;
  }
  .name {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .badge {
    font-size: 11.5px;
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    font-size: 12.5px;
    margin-top: 2px;
  }
  .actions {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    justify-content: flex-end;
  }
  .rename {
    display: flex;
    gap: 6px;
  }
  .rename input {
    flex: 1;
    min-width: 0;
  }
  .play {
    display: flex;
    gap: 6px;
  }
  .play select {
    max-width: 160px;
  }
  .hint {
    margin: 0 0 8px;
    font-size: 12.5px;
  }
  .backups li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }
  .warning {
    padding: 10px 14px;
    margin-bottom: 8px;
    border-radius: var(--radius);
    background: var(--warning-bg);
  }
  .warning p {
    margin: 4px 0 0;
  }
  .folders li {
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }
</style>
