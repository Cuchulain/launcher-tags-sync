# Upstream implementation references (read-only links)

Inspect upstream revisions and adjust if their formats changed. The observed private source export is Kvaesitso v1.41.0 with backup format 1.9; Quick Search export uses formatVersion 2. At investigation time, GitHub code-search result revisions included Kvaesitso `cb9ac2a44432d51deaf175129fa21961ae90c20e` and Quick Search `d89b40a38240cc6e2184fcc4d078bee09076437b` (do not assume those revisions remain current).

## Kvaesitso

- Backup/archive format and ZIP creation: https://github.com/MM2-0/Kvaesitso/blob/main/services/backup/src/main/java/de/mm20/launcher2/backup/BackupManager.kt
- Customizations backup JSON schema: https://github.com/MM2-0/Kvaesitso/blob/main/data/customattrs/src/main/java/de/mm20/launcher2/data/customattrs/CustomAttributesRepository.kt
- Tag custom attribute: https://github.com/MM2-0/Kvaesitso/blob/main/data/customattrs/src/main/java/de/mm20/launcher2/data/customattrs/CustomAttribute.kt
- Saved searchables and `favorites.*` data: https://github.com/MM2-0/Kvaesitso/blob/main/data/searchable/src/main/java/de/mm20/launcher2/searchable/SavableSearchableRepository.kt
- User guide for tags: https://github.com/MM2-0/Kvaesitso/blob/main/docs/docs/user-guide/concepts/tags.md

## Quick Search

- Backup JSON export/import: https://github.com/teja2495/quick-search/blob/main/app/src/main/java/com/tk/quicksearch/settings/settingsScreen/SettingsBackupManager.kt
- Backup preferences bridge: https://github.com/teja2495/quick-search/blob/main/app/src/main/java/com/tk/quicksearch/settings/settingsScreen/SettingsBackupStorageBridge.kt
- Nickname preference model: https://github.com/teja2495/quick-search/blob/main/app/src/main/java/com/tk/quicksearch/search/data/preferences/NicknamePreferences.kt
- Search customization indexed storage / restore: https://github.com/teja2495/quick-search/blob/main/app/src/main/java/com/tk/quicksearch/search/data/preferences/SearchCustomizationStore.kt
- Nickname list normalization: https://github.com/teja2495/quick-search/blob/main/app/src/main/java/com/tk/quicksearch/search/utils/NicknameUtils.kt
- Preference keys: https://github.com/teja2495/quick-search/blob/main/app/src/main/java/com/tk/quicksearch/search/data/preferences/BasePreferences.kt

## Reproducibility note

Verify the actual current upstream commit hashes before coding; branch `main` might change and documentation represents a snapshot of behavior observed on 2026-11-23. Preserve exact revision IDs in test notes once verified.

## Verified revisions (v0.1 implementation, 2026-11-23)

`git ls-remote … HEAD` at implementation time returned exactly the revisions above, and the
following files were read at those revisions:

| Project | Revision | Files checked | Finding used in v0.1 |
|---|---|---|---|
| Kvaesitso | `cb9ac2a44432d51deaf175129fa21961ae90c20e` | `CustomAttributesRepository.kt`, `BackupManager.kt`, `data/applications/…/LauncherApp.kt` | pages `customizations.%04d` with `{key,value,type}`; `meta.format` "X.Y" compatibility by major; app key `app://<package>:<class>` for the main profile and `app://<package>:<class>:<userSerial>` for other profiles |
| Quick Search | `d89b40a38240cc6e2184fcc4d078bee09076437b` | `NicknameUtils.kt`, `SettingsBackupManager.kt`, `SettingsBackupStorageBridge.kt`, `NicknamePreferences.kt`, `BasePreferences.kt` | aliases split on `,`, trimmed, blanks dropped, joined with `", "`; typed values `string/boolean/int/long/float/string_set`; `user_app_preferences` (`BasePreferences.PREFS_NAME`) is restored only when `selectedExportItems` is absent or contains `SETTINGS` |
