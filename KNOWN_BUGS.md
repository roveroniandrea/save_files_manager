# Known bugs & limitations

Format: ID · description · repro · workaround · status. Keep this file up to date (see `CLAUDE.md`).

| ID | Description | Repro | Workaround | Status |
|---|---|---|---|---|
| KB-001 | Steam Cloud may re-download the cloud copy and overwrite a swapped-in profile (or upload another profile's saves over the cloud copy). | Enable Steam Cloud for a game, switch profile, launch the game. | Disable Steam Cloud for that game (Properties → General → Steam Cloud). The app shows a warning when cloud sync is detected. | Open (by design, v1) |
| KB-002 | Games that store saves in the Windows registry are not supported. | Add a game whose saves live under `HKCU\...`. | None. | Open (not planned for v1) |
| KB-003 | Games that store saves inside their install directory may have them reset by Steam "Verify integrity of game files". | Verify files after switching profile. | Switch back to the profile after verifying; the profile copy is kept by the app. | Open |
| KB-004 | Changing the storage folder does not move existing profile data, so it is refused while any game is configured. | Settings → change storage folder with games present. | Remove all games first, or move the `games\` folder manually and edit `storage_root` in `config.json`. | Open (planned) |
| KB-005 | The swap unit is a whole folder: games that keep save files loose in a folder that also holds other files (e.g. directly in the game's install or exe folder) cannot be managed. Such folders are refused by the save path safety checks. | Add a game whose save path is its install folder. | None yet (file-level save paths could be added later). | Open (limitation) |
| KB-006 | Save folders containing symbolic links or junctions are not supported. | Put a junction inside a save folder and switch profile. | Remove the link. The switch fails safely during the backup step, before anything is modified. | Open (limitation) |
| KB-007 | Backups can only be restored for the profile that is currently active. | Try to restore a backup taken while another profile was active. | Switch to that profile first, then restore. | Open (by design, v1) |
| KB-008 | Save folders of manually added games are stored as absolute paths (no `<winAppData>`-style placeholders), so they break if the Windows user folder or drive letter changes. | Add a game, rename the Windows user / move the drive. | Edit the game and pick the folder again. | Open (placeholders planned with milestone 6) |
| KB-010 | Steam Cloud detection is a heuristic (cloud data present in `userdata`, per-app/global `CloudEnabled` flags): it can warn for a game whose cloud sync was turned off in a way the app does not read, or miss one that has never synced on this PC. | Turn cloud off for a game in Steam and open it in the app. | Ignore the banner if you know cloud sync is off. | Open (heuristic) |
| KB-011 | Steam's cached library icon is only 32×32, so Steam game icons look blurry at larger sizes; it is missing if Steam has not cached it yet. | Add a Steam game. | Edit the game and choose the executable's icon or a custom image. | Open |
| KB-012 | Save folders of Steam games are not suggested yet; they must be added by hand. | Add a Steam game. | Look the folder up (e.g. on PCGamingWiki) and add it. | Open (milestone 6) |
| KB-009 | Stored copies of a save folder removed from a game stay inside every profile until the profile (or game) is deleted, using disk space. | Edit a game, remove a save folder. | Delete unused profiles, or remove the `data\<save-path-id>` folder by hand. | Open (cleanup UI planned) |
