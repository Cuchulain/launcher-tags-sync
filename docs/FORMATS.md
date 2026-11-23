# Observed backup formats (23 Nov 2026)

## Kvaesitso source ZIP

Extension `.kvaesitso`; ZIP file (not a generic uncompressed JSON file). The observed backup uses a `meta` JSON member with `format: "1.9"`, `versionName: "1.41.0"`, and paginated JSON arrays `customizations.0000`, `customizations.0001`, ... (not just first page). Other members may include `settings.json`, `favorites.NNNN`, widget data and colors/themes. For **reading v0.1** only `customizations.*` is needed; `meta` may provide version diagnostics.

Tag record example (synthetic):

```json
{"key":"app://org.example.reader:org.example.reader.MainActivity","value":"čtení","type":"tag"}
```

Source keys can refer to the same package but different launch activities. Group by `org.example.reader`, not by the full `app://...` component key. Shortcuts look like `shortcut://...`: ignore in v0.1. Non-tag records such as `type: "icon"` or `type: "label"` are **not** tag assignments.

Do not assume values are escaped already for Quick Search, or that every source key is well-formed.

## Quick Search backup JSON

Extension `.quicksearch`; UTF-8 JSON, observed `formatVersion: 2`:

```json
{
  "formatVersion": 2,
  "exportedAtEpochMs": 1700000000000,
  "preferences": {
    "user_app_preferences": {
      "nickname_app_org.example.reader": {
        "type": "string",
        "value": "e-book, čtení"
      },
      "other_setting": {"type": "boolean", "value": true}
    }
  },
  "selectedExportItems": ["SETTINGS"]
}
```

Other root sections can include `notes`, `llmPersonalContexts`, optional sensitive keys, bookmark sections, etc. **Preserve all existing root sections, preference files, keys, typed values and unknown fields**. The result is a Quick Search import backup, not a minimal patch.

App nickname preference key: `nickname_app_<android-package>` for the main profile. Clone/work profile aliases may be `nickname_app_<android-package>:<userId>` — do not overwrite them when merging default-profile tags. The separate `nickname_app_shortcut_...` keys belong to app shortcuts, not normal apps.

As of the inspected Quick Search code, `SearchCustomizationStore.syncAfterImport` migrates restored nickname preferences to its indexed storage. Therefore edited export entries belong in `preferences.user_app_preferences` even if current runtime index is elsewhere.

## Format drift policy

Fail closed on unsupported source/target versions and malformed required objects; reject null or unexpected types rather than quietly writing a broken import. A ZIP may carry arbitrary extra members; ignore them, do not include unknown members in reports. Reconfirm upstream project source before introducing support for new formats.
