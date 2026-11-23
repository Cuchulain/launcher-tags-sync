# Privacy and robustness

Real Quick Search backups may contain search history, notes, personal context, shortcut intent URIs, or even exported API keys depending on selection. Real Kvaesitso backups reflect installed apps and personal organization. Treat all real backup files as **private**.

- This handoff contains only **synthetic** fixture files and numeric aggregate stats from a previously successful transfer. No real backups or converted output included.
- Work exclusively offline and locally by default. No telemetry, analytics, online dependency for conversion, remote uploads or automatic updates of source data.
- Do not print backup text in logs and do not record arbitrary JSON fields in reports. Include only counts and non-sensitive diagnostics.
- Never auto-overwrite source or destination; explicit `--force` only for an existing output file that is not one of the inputs.
- Validate ZIP entry count, total uncompressed size and per-member size limits; parse member contents without path extraction. Guard against zip-slip and zip bombs.
- Use `tempfile` and atomic rename where possible; cleanup partial output on error; handle symlink/path alias collisions to avoid source overwrites.
- Avoid destructive bidirectional sync until provenance/conflicts and rollback are designed and tested.
- Add `local-private/`, `.env`, backup outputs, and secret-bearing files to `.gitignore`. Warn users before asking for logs or samples.
- For any public issue or failing test, build a minimal synthetic reproducer; do not paste raw private backups.
