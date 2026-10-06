<script lang="ts">
  import { removeGame } from "$lib/api";
  import { t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";
  import type { Game } from "$lib/types";
  import Modal from "./Modal.svelte";

  let { game, onclose }: { game: Game; onclose: () => void } = $props();

  let deleteData = $state(false);

  async function confirm() {
    if (await app.run(() => removeGame(game.id, !deleteData))) onclose();
  }
</script>

<Modal title={t("remove.title", { name: game.name })} {onclose}>
  <p>{t("remove.body")}</p>
  <label class="check">
    <input type="checkbox" bind:checked={deleteData} />
    {t("remove.deleteData")}
  </label>

  {#snippet footer()}
    <button type="button" onclick={onclose}>{t("common.cancel")}</button>
    <button type="button" class="primary" disabled={app.busy} onclick={confirm}>{t("remove.confirm")}</button>
  {/snippet}
</Modal>

<style>
  p {
    margin-top: 0;
  }
  .check {
    display: flex;
    gap: 8px;
    align-items: flex-start;
  }
</style>
