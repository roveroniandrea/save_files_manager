# Known bugs & limitations

Format: ID · description · repro · workaround · status. Keep this file up to date (see `CLAUDE.md`).

| ID | Description | Repro | Workaround | Status |
|---|---|---|---|---|
| KB-001 | Steam Cloud may re-download the cloud copy and overwrite a swapped-in profile (or upload another profile's saves over the cloud copy). | Enable Steam Cloud for a game, switch profile, launch the game. | Disable Steam Cloud for that game (Properties → General → Steam Cloud). The app shows a warning when cloud sync is detected. | Open (by design, v1) |
| KB-002 | Games that store saves in the Windows registry are not supported. | Add a game whose saves live under `HKCU\...`. | None. | Open (not planned for v1) |
| KB-003 | Games that store saves inside their install directory may have them reset by Steam "Verify integrity of game files". | Verify files after switching profile. | Switch back to the profile after verifying; the profile copy is kept by the app. | Open |
