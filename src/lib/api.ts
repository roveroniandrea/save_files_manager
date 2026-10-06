// Typed wrappers over Tauri commands (src-tauri/src/commands.rs).
import { invoke } from "@tauri-apps/api/core";
import { hasKey, t } from "./i18n";
import type {
  AppError,
  BackupInfo,
  CloudStatus,
  Config,
  GameInput,
  IconChoice,
  ManifestStatus,
  NewProfileSource,
  ProfileStats,
  RecoveryReport,
  Settings,
  SteamScan,
  SuggestQuery,
  Suggestions,
  SwapOptions,
  Uuid,
} from "./types";

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

/** Adds a game; `steamAppid` set = an installed Steam game. */
export const addGame = (input: GameInput, firstProfileName: string, steamAppid: number | null = null) =>
  invoke<Config>("add_game", { input, firstProfileName, steamAppid });

export const updateGame = (gameId: Uuid, input: GameInput) =>
  invoke<Config>("update_game", { gameId, input });

/** Removes the game from the app; live save folders are never touched. */
export const removeGame = (gameId: Uuid, keepData: boolean) =>
  invoke<Config>("remove_game", { gameId, keepData });

export const createProfile = (gameId: Uuid, name: string, source: NewProfileSource) =>
  invoke<Config>("create_profile", { gameId, name, source });

export const renameProfile = (gameId: Uuid, profileId: Uuid, name: string) =>
  invoke<Config>("rename_profile", { gameId, profileId, name });

export const deleteProfile = (gameId: Uuid, profileId: Uuid) =>
  invoke<Config>("delete_profile", { gameId, profileId });

export const profileStats = (gameId: Uuid) => invoke<ProfileStats[]>("profile_stats", { gameId });

/** The game's icon as a `data:` URL, or null. */
export const getIcon = (gameId: Uuid) => invoke<string | null>("get_icon", { gameId });

export const previewIcon = (choice: IconChoice, exePath: string | null, steamAppid: number | null = null) =>
  invoke<string | null>("preview_icon", { choice, exePath, steamAppid });

/** Installed Steam games (tools filtered out). */
export const steamScan = () => invoke<SteamScan>("steam_scan");

/** Steam Cloud status of a Steam game; null for other games or without Steam. */
export const steamCloudStatus = (gameId: Uuid) => invoke<CloudStatus | null>("steam_cloud_status", { gameId });

export const manifestStatus = () => invoke<ManifestStatus>("manifest_status");

/** Checks for a newer Ludusavi manifest now. */
export const refreshManifest = () => invoke<ManifestStatus>("refresh_manifest");

/** Save folder suggestions from the Ludusavi manifest (waits for a running download). */
export const suggestSavePaths = (query: SuggestQuery) => invoke<Suggestions>("suggest_save_paths", { query });

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
