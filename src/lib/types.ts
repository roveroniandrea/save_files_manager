// Mirrors the Rust structs in src-tauri/src/config.rs and error.rs. Keep in sync.

export type Uuid = string;
/** ISO-8601 UTC timestamp. */
export type Timestamp = string;

export type Theme = "system" | "light" | "dark";

export interface Settings {
  storage_root: string | null;
  backup_limit: number;
  include_config: boolean;
  theme: Theme;
}

export type GameSource =
  | { kind: "steam"; appid: number; install_dir: string }
  | { kind: "manual" };

export interface SavePath {
  template: string;
  resolved: string;
}

export interface Profile {
  id: Uuid;
  name: string;
  created_at: Timestamp;
  last_used_at: Timestamp | null;
}

export interface Game {
  id: Uuid;
  name: string;
  source: GameSource;
  exe_path: string | null;
  icon: string | null;
  save_paths: SavePath[];
  profiles: Profile[];
  active_profile_id: Uuid | null;
  backup_limit: number | null;
}

export interface Config {
  version: number;
  settings: Settings;
  games: Game[];
}

/** Error returned by every command (`AppError` in Rust). */
export interface AppError {
  kind: string;
  message: string;
}

export interface SwapOptions {
  /** Proceed although live save folders are empty (after `live_saves_empty`). */
  allow_empty_live?: boolean;
}

export type BackupReason = "switch" | "restore";

export interface BackupInfo {
  /** Folder name, used as id. */
  id: string;
  profile_id: Uuid;
  created_at: Timestamp;
  reason: BackupReason;
  save_path_count: number;
}

export type RecoveryOutcome = "rolled_back" | "completed" | "failed";

export interface RecoveryReport {
  game_id: Uuid;
  game_name: string | null;
  outcome: RecoveryOutcome;
  error: string | null;
}
