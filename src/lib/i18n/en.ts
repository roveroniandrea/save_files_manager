// English message catalog: the reference catalog. Every other locale must
// provide the same keys (enforced by the `Messages` type).
// Placeholders: `{name}`. Plurals: `<key>_one` / `<key>_other`, used via `tn()`.

export const en = {
  "app.title": "Save Files Manager",
  "app.loading": "Loading…",

  "common.cancel": "Cancel",
  "common.save": "Save",
  "common.close": "Close",
  "common.details": "Details",

  "library.add": "Add game",
  "library.empty": "No games yet. Add a game to start managing its save profiles.",
  "library.noSelection": "Select a game, or add one.",

  "game.edit": "Edit",
  "game.remove": "Remove",
  "game.profiles": "Profiles",
  "game.saveFolders": "Save folders",
  "game.executable": "Executable",
  "game.noExecutable": "Not set",
  "game.profileCount_one": "{count} profile",
  "game.profileCount_other": "{count} profiles",
  "game.steamAppId": "Steam app {appid}",
  "game.installFolder": "Install folder",

  "steam.pickTitle": "Add game",
  "steam.scanning": "Looking for installed Steam games…",
  "steam.notFound": "Steam was not found on this PC. You can still add any game with “Other game…”.",
  "steam.noGames": "No installed Steam games were found.",
  "steam.search": "Search installed Steam games",
  "steam.noMatch": "No game matches the search.",
  "steam.alreadyAdded": "Already added",
  "steam.otherGame": "Other game…",

  "cloud.title": "Steam Cloud is on for this game",
  "cloud.body":
    "Steam Cloud keeps a copy of the saves of {accounts} and syncs it when the game starts or closes. After a profile switch, Steam may overwrite the swapped saves with its cloud copy, or ask which copy to keep — choose the local one.",
  "cloud.howTo":
    "To avoid this, turn cloud saves off for this game: in the Steam library right-click the game → Properties → General → turn off “Keep game saves in the Steam Cloud”.",

  "profile.active": "Active",
  "profile.switch": "Switch to this profile",
  "profile.rename": "Rename",
  "profile.delete": "Delete",
  "profile.lastUsed": "Last used {date}",
  "profile.neverUsed": "Never used",
  "profile.modified": "Saves modified {date}",
  "profile.noSaves": "No saves",
  "profile.new": "New profile",
  "profile.name": "Name",
  "profile.startFrom": "Start from",
  "profile.fresh": "Empty (the game starts a new save)",
  "profile.cloneOf": "Copy of “{name}”",
  "profile.create": "Create",
  "profile.deleteTitle": "Delete profile",
  "profile.deleteConfirm": "Delete the profile “{name}” and its stored saves? This cannot be undone.",

  "switch.emptyTitle": "Empty save folder",
  "switch.emptyConfirm":
    "The save folders of “{game}” are empty, but the active profile “{profile}” has stored saves.\n\nIf you continue, the stored saves of “{profile}” are replaced by the empty folders (a backup is still taken first). Continue?",
  "switch.continue": "Continue",
  "switch.done": "“{game}” now uses the profile “{profile}”.",

  "editor.addTitle": "Add game",
  "editor.editTitle": "Edit game",
  "editor.name": "Name",
  "editor.executable": "Game executable (optional)",
  "editor.browse": "Browse…",
  "editor.clear": "Clear",
  "editor.icon": "Icon",
  "editor.iconKeep": "Current",
  "editor.iconExe": "From the executable",
  "editor.iconSteam": "From Steam",
  "editor.steamApp": "Steam game",
  "editor.steamFoldersHint":
    "Save folders are not detected automatically yet: add them yourself (PCGamingWiki lists them for most games).",
  "editor.iconCustom": "Image…",
  "editor.iconNone": "None",
  "editor.saveFolders": "Save folders",
  "editor.saveFoldersHint":
    "The folders where the game keeps its saves. They are swapped together when you switch profile.",
  "editor.addFolder": "Add folder…",
  "editor.removeFolder": "Remove",
  "editor.noFolders": "Add at least one save folder.",
  "editor.firstProfile": "Name of the first profile",
  "editor.firstProfileHint": "The saves currently in these folders become this profile.",
  "editor.firstProfileDefault": "Default",
  "editor.backupLimit": "Backups to keep",
  "editor.backupLimitDefault": "Default ({count})",
  "editor.pickExe": "Choose the game executable",
  "editor.pickFolder": "Choose a save folder",
  "editor.pickImage": "Choose an icon image",
  "editor.programs": "Programs",
  "editor.images": "Images",

  "remove.title": "Remove “{name}”",
  "remove.body": "The game is removed from Save Files Manager. Its current save folders are not touched.",
  "remove.deleteData":
    "Also delete its stored profiles and backups (otherwise they are kept in the “_orphaned” folder)",
  "remove.confirm": "Remove",

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
  "error.name_taken": "A profile with this name already exists.",
  "error.empty_name": "The name cannot be empty.",
  "error.profile_active": "The active profile cannot be deleted. Switch to another profile first.",
  "error.backup_not_found": "The backup was not found.",
  "error.invalid_save_path": "A save folder is not valid for swapping.",
  "error.operation_in_progress": "Another operation on this game is in progress or needs recovery.",
  "error.live_saves_empty":
    "The game's save folder is empty, but the active profile has saved data. Continuing will replace that data with an empty folder.",
  "error.rollback_failed":
    "The operation failed and the save folders could not be restored automatically. Your data is safe in the backup; recovery will be retried when the app starts.",
  "error.game_already_added": "This game is already in the list.",
  "error.steam_not_found": "Steam was not found on this PC.",
  "error.steam_app_not_found": "This Steam game is not installed (anymore).",
  "error.other": "Something went wrong.",
  "error.unknown": "Unexpected error.",
} as const;

export type MessageKey = keyof typeof en;
export type Messages = Record<MessageKey, string>;
