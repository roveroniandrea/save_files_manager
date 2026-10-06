<script lang="ts">
  import { untrack } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { addGame, getIcon, previewIcon, updateGame } from "$lib/api";
  import { t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";
  import type { Game, IconChoice, SavePathInput } from "$lib/types";
  import Modal from "./Modal.svelte";

  /** `game` set = edit mode, otherwise add mode. */
  let { game, onclose }: { game?: Game; onclose: () => void } = $props();

  // Snapshot of the game being edited: the form is not reset by config updates.
  const initial = untrack(() => (game ? $state.snapshot(game) : undefined));

  let name = $state(initial?.name ?? "");
  let exePath = $state<string | null>(initial?.exe_path ?? null);
  let folders = $state<SavePathInput[]>(initial?.save_paths.map((p) => ({ id: p.id, path: p.resolved })) ?? []);
  let firstProfile = $state(t("editor.firstProfileDefault"));
  let backupLimit = $state<number | null>(initial?.backup_limit ?? null);
  let icon = $state<IconChoice>(initial ? { kind: "keep" } : { kind: "exe" });
  let iconPreview = $state<string | null>(null);

  const defaultLimit = $derived(app.config?.settings.backup_limit ?? 5);
  const canSave = $derived(name.trim() !== "" && folders.length > 0 && (initial || firstProfile.trim() !== ""));

  $effect(() => {
    const choice = $state.snapshot(icon);
    const exe = exePath;
    let cancelled = false;
    const load =
      choice.kind === "keep" && initial ? getIcon(initial.id) : choice.kind === "none" ? null : previewIcon(choice, exe);
    if (!load) {
      iconPreview = null;
      return;
    }
    load
      .then((url) => {
        if (!cancelled) iconPreview = url;
      })
      .catch(() => {
        if (!cancelled) iconPreview = null;
      });
    return () => {
      cancelled = true;
    };
  });

  async function pickExe() {
    const path = await open({
      title: t("editor.pickExe"),
      filters: [{ name: t("editor.programs"), extensions: ["exe"] }],
    });
    if (path) {
      exePath = path;
      if (!name.trim()) name = path.split(/[\\/]/).pop()!.replace(/\.exe$/i, "");
    }
  }

  async function pickImage() {
    const path = await open({
      title: t("editor.pickImage"),
      filters: [{ name: t("editor.images"), extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico"] }],
    });
    if (path) icon = { kind: "custom", path };
  }

  async function addFolder() {
    const path = await open({ title: t("editor.pickFolder"), directory: true });
    if (path && !folders.some((f) => f.path.toLowerCase() === path.toLowerCase())) {
      folders.push({ path });
    }
  }

  async function changeFolder(index: number) {
    const path = await open({ title: t("editor.pickFolder"), directory: true, defaultPath: folders[index].path });
    if (path) folders[index].path = path;
  }

  async function submit(event: Event) {
    event.preventDefault();
    if (!canSave) return;
    const input = {
      name,
      exe_path: exePath,
      save_paths: $state.snapshot(folders),
      backup_limit: backupLimit,
      icon: $state.snapshot(icon),
    };
    const before = new Set(app.config?.games.map((g) => g.id));
    const ok = await app.run(() => (initial ? updateGame(initial.id, input) : addGame(input, firstProfile)));
    if (ok) {
      if (!initial) app.selectedId = app.config?.games.find((g) => !before.has(g.id))?.id ?? app.selectedId;
      onclose();
    }
  }
</script>

<Modal title={initial ? t("editor.editTitle") : t("editor.addTitle")} {onclose}>
  <form id="game-editor" onsubmit={submit}>
    <label>
      {t("editor.name")}
      <input type="text" bind:value={name} required />
    </label>

    <div class="field">
      <span>{t("editor.executable")}</span>
      <div class="row">
        <span class="path grow">{exePath ?? t("game.noExecutable")}</span>
        <button type="button" onclick={pickExe}>{t("editor.browse")}</button>
        {#if exePath}
          <button type="button" onclick={() => (exePath = null)}>{t("editor.clear")}</button>
        {/if}
      </div>
    </div>

    <div class="field">
      <span>{t("editor.icon")}</span>
      <div class="row">
        <span class="preview">
          {#if iconPreview}<img src={iconPreview} alt="" width="40" height="40" />{/if}
        </span>
        <div class="choices">
          {#if initial}
            <label class="inline">
              <input type="radio" checked={icon.kind === "keep"} onchange={() => (icon = { kind: "keep" })} />
              {t("editor.iconKeep")}
            </label>
          {/if}
          <label class="inline">
            <input
              type="radio"
              checked={icon.kind === "exe"}
              disabled={!exePath}
              onchange={() => (icon = { kind: "exe" })}
            />
            {t("editor.iconExe")}
          </label>
          <label class="inline">
            <input type="radio" checked={icon.kind === "none"} onchange={() => (icon = { kind: "none" })} />
            {t("editor.iconNone")}
          </label>
          <button type="button" class:selected={icon.kind === "custom"} onclick={pickImage}>
            {t("editor.iconCustom")}
          </button>
        </div>
      </div>
    </div>

    <div class="field">
      <span>{t("editor.saveFolders")}</span>
      <p class="muted hint">{t("editor.saveFoldersHint")}</p>
      {#each folders as folder, i (folder.id ?? folder.path)}
        <div class="row folder">
          <button type="button" class="path grow link" onclick={() => changeFolder(i)}>{folder.path}</button>
          <button type="button" class="danger" onclick={() => folders.splice(i, 1)}>
            {t("editor.removeFolder")}
          </button>
        </div>
      {:else}
        <p class="muted">{t("editor.noFolders")}</p>
      {/each}
      <button type="button" onclick={addFolder}>{t("editor.addFolder")}</button>
    </div>

    {#if !initial}
      <label>
        {t("editor.firstProfile")}
        <input type="text" bind:value={firstProfile} required />
        <span class="muted hint">{t("editor.firstProfileHint")}</span>
      </label>
    {:else}
      <label>
        {t("editor.backupLimit")}
        <select
          value={backupLimit ?? ""}
          onchange={(e) => {
            const v = e.currentTarget.value;
            backupLimit = v === "" ? null : Number(v);
          }}
        >
          <option value="">{t("editor.backupLimitDefault", { count: defaultLimit })}</option>
          {#each [1, 2, 3, 5, 10, 20, 50] as n (n)}
            <option value={n}>{n}</option>
          {/each}
        </select>
      </label>
    {/if}
  </form>

  {#snippet footer()}
    <button type="button" onclick={onclose}>{t("common.cancel")}</button>
    <button type="submit" form="game-editor" class="primary" disabled={!canSave || app.busy}>
      {t("common.save")}
    </button>
  {/snippet}
</Modal>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  label > input,
  label > select {
    display: block;
    width: 100%;
    margin-top: 4px;
  }
  label.inline {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  label.inline > input {
    display: inline;
    width: auto;
    margin: 0;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
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
  .hint {
    margin: 0;
    font-size: 12.5px;
  }
  .choices {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 14px;
  }
  .choices .selected {
    border-color: var(--accent);
  }
  .preview {
    width: 40px;
    height: 40px;
    border-radius: 6px;
    background: var(--surface-2);
    flex-shrink: 0;
  }
  .link {
    text-align: left;
    background: var(--surface-2);
  }
</style>
