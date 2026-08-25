# Pre-1.0 database release boundary

The pre-1.0 release ships a single SQLite baseline migration at version 1.
Development databases from before the current baseline are not supported upgrade
inputs.

## Release reset requirement

Before validating the release, stop the app and remove each development
database created before the baseline. Remove its `-wal` and `-shm` sidecars as
well. Launching the app against the same path recreates the file and applies the
baseline through the embedded migrator. Use `PV_DB_URL` to make the path
explicit in automated checks.

Do not ship a release validation result from a database that was upgraded from
a pre-baseline development file. Run the fresh-database first-run checks after
the reset.

## Baseline migration contract

Pre-1.0 schema and data changes edit the baseline migration in place at version
1. There is no `0002` or later, so there is no upgrade from a preceding migration
to validate. Release validation covers the fresh-database path only (bead
`adr-1`).

Every baseline edit invalidates existing development databases, and the app does
not back them up. See
[`docs/development/database-baseline-and-migrations.md`](../development/database-baseline-and-migrations.md).
