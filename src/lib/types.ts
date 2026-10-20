// Mirrors the Rust structs in src-tauri/src/config.rs and error.rs. Keep in sync.

export type Uuid = string;
/** ISO-8601 UTC timestamp. */
export type Timestamp = string;

export type Theme = "system" | "light" | "dark";
export type Language = "system" | "en" | "it";

export interface Settings {
  storage_root: string | null;
  backup_limit: number;
  include_config: boolean;
  theme: Theme;
  language: Language;
}

export type GameSource =
  | { kind: "steam"; appid: number; install_dir: string }
  | { kind: "manual" };

export interface SavePath {
  /** Stable id: stored profile data is keyed by it. */
  id: Uuid;
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
  save_path_ids: Uuid[];
}

export type RecoveryOutcome = "rolled_back" | "completed" | "failed";

export interface RecoveryReport {
  game_id: Uuid;
  game_name: string | null;
  outcome: RecoveryOutcome;
  error: string | null;
}

export type IconChoice =
  | { kind: "keep" }
  | { kind: "exe" }
  | { kind: "steam" }
  | { kind: "custom"; path: string }
  | { kind: "none" };

export interface SavePathInput {
  /** Existing save path being edited; omitted for a new one. */
  id?: Uuid;
  path: string;
}

/** Editable fields of a game (`games::GameInput`). */
export interface GameInput {
  name: string;
  exe_path: string | null;
  save_paths: SavePathInput[];
  backup_limit: number | null;
  icon: IconChoice;
}

export type NewProfileSource = { kind: "fresh" } | { kind: "clone"; profile_id: Uuid };

export interface ProfileStats {
  profile_id: Uuid;
  size_bytes: number;
  last_modified: Timestamp | null;
}

/** An installed Steam game (`steam::ScannedGame`). */
export interface ScannedGame {
  appid: number;
  name: string;
  install_dir: string;
  /** Steam library icon as a `data:` URL. */
  icon: string | null;
}

export interface SteamScan {
  /** Steam install folder; null when Steam was not found. */
  root: string | null;
  games: ScannedGame[];
}

export interface CloudAccount {
  account_id: number;
  persona_name: string | null;
}

export interface CloudStatus {
  /** Steam accounts on this PC whose Steam Cloud syncs this game's saves. */
  accounts: CloudAccount[];
}

/** Ludusavi manifest cache (`manifest::ManifestStatus`). */
export interface ManifestStatus {
  updating: boolean;
  /** Games in the cached index; 0 = not downloaded yet. */
  games: number;
  checked_at: Timestamp | null;
  updated_at: Timestamp | null;
  /** English detail of the last failed check. */
  error: string | null;
}

export interface SuggestQuery {
  name: string;
  steam_appid: number | null;
  exe_path: string | null;
  /** Database entry picked by the user; used instead of the automatic match. */
  entry?: string | null;
}

export interface Suggestion {
  /** Placeholder form, e.g. `<winAppData>/Studio/Game`. */
  template: string;
  path: string;
  exists: boolean;
  save: boolean;
  config: boolean;
  /** English reason why it cannot be a save folder, if so. */
  problem: string | null;
}

export interface Suggestions {
  status: ManifestStatus;
  /** Manifest entry used; null = the game was not found. */
  matched: string | null;
  items: Suggestion[];
  /** Similar database entry names when nothing matched ("did you mean"), best first. */
  similar: string[];
}
