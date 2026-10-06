// English message catalog: the reference catalog. Every other locale must
// provide the same keys (enforced by the `Messages` type).
// Placeholders: `{name}`. Plurals: `<key>_one` / `<key>_other`, used via `tn()`.

export const en = {
  "app.title": "Save Files Manager",
  "app.loading": "Loading…",

  "library.gameCount_one": "{count} game configured.",
  "library.gameCount_other": "{count} games configured.",

  "recovery.restored":
    "An interrupted operation on “{game}” was recovered: the save folders were restored from the backup taken just before it.",
  "recovery.completed":
    "An interrupted profile switch on “{game}” had already copied all files; it was completed.",
  "recovery.failed": "Recovery of an interrupted operation on “{game}” failed: {detail}",

  // One entry per `AppError` kind (src-tauri/src/error.rs).
  "error.io": "A file operation failed.",
  "error.config_parse": "The configuration file is damaged and could not be read.",
  "error.unsupported_config_version": "The configuration was created by a newer version of the app.",
  "error.invalid_config": "The configuration is not valid.",
  "error.no_app_data_dir": "The application data folder could not be found.",
  "error.game_not_found": "The game was not found.",
  "error.profile_not_found": "The profile was not found.",
  "error.backup_not_found": "The backup was not found.",
  "error.invalid_save_path": "A save folder is not valid for swapping.",
  "error.operation_in_progress": "Another operation on this game is in progress or needs recovery.",
  "error.live_saves_empty":
    "The game's save folder is empty, but the active profile has saved data. Continuing will replace that data with an empty folder.",
  "error.rollback_failed":
    "The operation failed and the save folders could not be restored automatically. Your data is safe in the backup; recovery will be retried when the app starts.",
  "error.other": "Something went wrong.",
  "error.unknown": "Unexpected error.",
} as const;

export type MessageKey = keyof typeof en;
export type Messages = Record<MessageKey, string>;
