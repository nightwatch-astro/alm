# Pre-1.0 database baseline

This guide defines the development database boundary for the SQLite store.

## Unsupported databases

Any database created before the pre-1.0 baseline is unsupported. Recreate it
from the baseline migration shipped by the checkout under test. Do not edit
`_sqlx_migrations` or copy tables between schema generations to preserve test
data.

The reset is destructive to the selected SQLite file. Preserve fixtures or
exports outside the database file before removing it.

## Reset procedure

1. Stop the desktop app and any process using the database.
2. Determine the database URL. `PV_DB_URL` takes precedence. Without it,
   the app creates `alm.db` under Tauri's app-data directory.
3. Remove the selected file and its SQLite sidecars (`<file>-wal` and
   `<file>-shm`) if present.
4. Start the app or test command. `Database::migrate` creates the file and
   applies the embedded baseline migration.
5. Confirm the first-run flow or test fixture creates the expected seed data.

For the Windows native development checkout, the default file is:

```powershell
$db = "$env:APPDATA\dev.astro-plan.astro-library-manager\alm.db"
Remove-Item -LiteralPath $db, "$db-wal", "$db-shm" -Force -ErrorAction SilentlyContinue
```

For an explicit development URL, remove the path named by `PV_DB_URL` after
stripping the `sqlite://` prefix and query string. Use a path-specific remove
command; never delete an entire app-data directory.

## Single editable baseline

The pre-1.0 schema is stated once, in
`crates/persistence/core/migrations/0001_initial_schema.sql`, at migration
version 1. Schema and seed changes edit that file in place. The baseline is
version 1 and stays version 1 until 1.0; there is no `0002` or later. This
supersedes the earlier freeze-and-append rule (bead `adr-1`).

Each schema change follows this workflow:

1. Edit `0001_initial_schema.sql`.
2. Update repository queries and tests in the same change.
3. Delete your development database (see below) and run the persistence tests
   against a fresh one.
4. Review the SQL in the diff before merging. Once `astro-plan-0zog4` lands,
   reseal the schema-shape snapshot by reading its diff, not by blessing it.

### Every schema edit destroys your development database

Any development database created before the edit is unusable. Delete the file
and its `-wal` and `-shm` sidecars yourself. The app does not back it up first,
because the version-1 edit is invisible to the pending-migration check:

- `has_pending_migrations` returns `applied_count < total`
  (`crates/persistence/core/src/lib.rs:180-185`). With three applied rows from
  an older chain and one embedded migration, `3 < 1` is false, so it reports no
  pending work.
- The desktop shell therefore skips its pre-migration `VACUUM INTO` backup
  (`apps/desktop/src-tauri/src/lib.rs:542`, skip condition at `:543`, the
  `backup_to` call at `:554`). Nobody gets a `.bak`.
- `MIGRATOR.run` then fails on the version-1 checksum mismatch, classified by
  `persistence_core::migration_divergence_detail`
  (`crates/persistence/core/src/lib.rs:257`) and surfaced at boot
  (`apps/desktop/src-tauri/src/lib.rs:631`).

The failure is loud and does not corrupt the file, but it is not recoverable in
place. That boot error means delete the database. It is not a bug report.

### Drift detection

Runtime tamper detection is unchanged and remains the stronger control: sqlx
compares the recorded `_sqlx_migrations` checksum against the embedded migration
on every `run()`.

Compile-time detection of an edit to the baseline file is gone. It was a 48-byte
SHA-384 literal pinning the file bytes. Its replacement, a schema-shape
snapshot, is deferred to bead `astro-plan-0zog4` and ships immediately after
this change; until then the baseline has no compile-time drift control. The
snapshot will not catch seed content beyond the filters it checks, SQL
behaviour, a blind reseal, or formatting-only edits.
