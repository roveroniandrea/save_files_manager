<script lang="ts">
  import { t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";
</script>

<div class="toasts" aria-live="polite">
  {#each app.toasts as toast (toast.id)}
    <div class="toast {toast.kind}" role={toast.kind === "error" ? "alert" : "status"}>
      <div class="text">
        <p>{toast.text}</p>
        {#if toast.detail && toast.detail !== toast.text}
          <details>
            <summary>{t("common.details")}</summary>
            <p class="path">{toast.detail}</p>
          </details>
        {/if}
      </div>
      <button class="close" aria-label={t("common.close")} onclick={() => app.dismiss(toast.id)}>×</button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: min(420px, calc(100vw - 32px));
    z-index: 10;
  }
  .toast {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding: 10px 12px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    border-left: 4px solid var(--success);
    background: var(--surface);
    box-shadow: 0 4px 16px rgb(0 0 0 / 0.15);
  }
  .toast.error {
    border-left-color: var(--danger);
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  p {
    margin: 0;
    white-space: pre-line;
  }
  details {
    margin-top: 4px;
    color: var(--muted);
  }
  .close {
    border: none;
    background: none;
    padding: 0 4px;
    font-size: 18px;
    line-height: 1;
  }
</style>
