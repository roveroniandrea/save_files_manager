// Global UI state: the config snapshot, selection, busy flag and toasts.
import { errorDetail, errorMessage, getConfig } from "./api";
import { applyLanguage } from "./i18n";
import type { Config, Game, UpdateInfo, Uuid } from "./types";

export interface Toast {
  id: number;
  kind: "info" | "error";
  text: string;
  detail?: string;
}

class AppState {
  config = $state<Config | null>(null);
  selectedId = $state<Uuid | null>(null);
  /** Main panel: the selected game, or the settings page. */
  view = $state<"game" | "settings">("game");
  /** A long-running command is in progress (buttons are disabled). */
  busy = $state(false);
  toasts = $state<Toast[]>([]);
  /** Fatal load error (config unreadable). */
  loadError = $state<{ summary: string; detail: string } | null>(null);
  /** A newer app version waiting for the user's confirmation (update dialog). */
  update = $state<UpdateInfo | null>(null);
  #nextToast = 0;

  get selected(): Game | null {
    return this.config?.games.find((g) => g.id === this.selectedId) ?? null;
  }

  async load() {
    try {
      this.setConfig(await getConfig());
    } catch (e) {
      this.loadError = { summary: errorMessage(e), detail: errorDetail(e) };
    }
  }

  setConfig(config: Config) {
    this.config = config;
    // Theme override for app.css ("system" follows the OS).
    document.documentElement.dataset.theme = config.settings.theme;
    applyLanguage(config.settings.language);
    if (!config.games.some((g) => g.id === this.selectedId)) {
      this.selectedId = config.games[0]?.id ?? null;
    }
  }

  /** Shows a toast. Info toasts disappear after a few seconds unless `sticky`. */
  toast(kind: Toast["kind"], text: string, detail?: string, sticky = false) {
    const id = this.#nextToast++;
    this.toasts.push({ id, kind, text, detail });
    if (kind === "info" && !sticky) setTimeout(() => this.dismiss(id), 5000);
  }

  dismiss(id: number) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  /**
   * Runs a command that returns the updated config. Errors become toasts
   * (unless `onError` handles them and returns true). Returns true on success.
   */
  async run(
    command: () => Promise<Config>,
    onError?: (e: unknown) => boolean | Promise<boolean>,
  ): Promise<boolean> {
    this.busy = true;
    try {
      this.setConfig(await command());
      return true;
    } catch (e) {
      if (!(onError && (await onError(e)))) this.toast("error", errorMessage(e), errorDetail(e));
      return false;
    } finally {
      this.busy = false;
    }
  }
}

export const app = new AppState();
