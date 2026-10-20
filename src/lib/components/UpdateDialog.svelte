<script lang="ts">
  import { relaunch } from "@tauri-apps/plugin-process";
  import { errorDetail, errorMessage, installUpdate } from "$lib/api";
  import { formatDate } from "$lib/format";
  import { t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";
  import type { UpdateInfo } from "$lib/types";
  import Modal from "./Modal.svelte";

  let { update }: { update: UpdateInfo } = $props();

  type Phase = "ask" | "downloading" | "waiting" | "installing";
  let phase = $state<Phase>("ask");
  let downloaded = $state(0);
  let total = $state<number | null>(null);

  const percent = $derived(total ? Math.min(100, Math.round((downloaded / total) * 100)) : null);
  const working = $derived(phase !== "ask");

  function close() {
    if (!working) app.update = null;
  }

  async function install() {
    phase = "downloading";
    try {
      await installUpdate((e) => {
        if (e.event === "progress") {
          downloaded = e.downloaded;
          total = e.total;
        } else if (e.event === "downloaded") {
          phase = "waiting";
        } else {
          phase = "installing";
        }
      });
      // Only reached where the installer does not exit the app (not Windows).
      await relaunch();
    } catch (e) {
      phase = "ask";
      app.toast("error", errorMessage(e), errorDetail(e));
    }
  }
</script>

<Modal title={t("update.title")} onclose={close} locked={working}>
  <p>{t("update.body", { version: update.version, current: update.current_version })}</p>
  {#if update.date}<p class="muted">{t("update.published", { date: formatDate(update.date) })}</p>{/if}
  {#if update.notes}
    <h3>{t("update.notes")}</h3>
    <pre class="notes">{update.notes}</pre>
  {/if}
  {#if phase === "downloading"}
    <p>{percent === null ? t("update.downloading") : t("update.downloadingPercent", { percent })}</p>
    <progress max="100" value={percent ?? undefined}></progress>
  {:else if phase === "waiting"}
    <p>{t("update.waiting")}</p>
  {:else if phase === "installing"}
    <p>{t("update.installing")}</p>
  {:else}
    <p class="muted hint">{t("update.hint")}</p>
  {/if}

  {#snippet footer()}
    <button onclick={close} disabled={working}>{t("update.later")}</button>
    <button class="primary" onclick={install} disabled={working}>{t("update.install")}</button>
  {/snippet}
</Modal>

<style>
  h3 {
    font-size: 14px;
    margin: 14px 0 6px;
  }
  .notes {
    max-height: 240px;
    overflow: auto;
    margin: 0;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    white-space: pre-wrap;
    font-family: inherit;
    font-size: 13px;
  }
  progress {
    width: 100%;
  }
  .hint {
    font-size: 12.5px;
  }
</style>
