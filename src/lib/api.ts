// Typed wrappers over Tauri commands (src-tauri/src/commands.rs).
import { invoke } from "@tauri-apps/api/core";
import { hasKey, t } from "./i18n";
import type { AppError, BackupInfo, Config, RecoveryReport, Settings, SwapOptions, Uuid } from "./types";

export const getConfig = () => invoke<Config>("get_config");

export const updateSettings = (settings: Settings) =>
  invoke<Config>("update_settings", { settings });

export const switchProfile = (gameId: Uuid, profileId: Uuid, options?: SwapOptions) =>
  invoke<Config>("switch_profile", { gameId, profileId, options });

export const listBackups = (gameId: Uuid) => invoke<BackupInfo[]>("list_backups", { gameId });

export const restoreBackup = (gameId: Uuid, backupId: string) =>
  invoke<Config>("restore_backup", { gameId, backupId });

/** Startup crash-recovery results; returned only once per app run. */
export const takeRecoveryReports = () => invoke<RecoveryReport[]>("take_recovery_reports");

export function isAppError(e: unknown): e is AppError {
  return !!e && typeof e === "object" && "kind" in e && "message" in e;
}

/** Localized summary of a rejected command. */
export function errorMessage(e: unknown): string {
  if (isAppError(e)) {
    const key = `error.${e.kind}`;
    return hasKey(key) ? t(key) : e.message;
  }
  return t("error.unknown");
}

/** Technical (English) detail of a rejected command, for a "details" section. */
export function errorDetail(e: unknown): string {
  return isAppError(e) ? e.message : String(e);
}
