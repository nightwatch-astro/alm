# TinySpec: One editable database baseline before 1.0

**Branch**: fix/baseline-consolidate-r2
**Date**: 2026-08-23
**Complexity**: 3 (TinySpec route)
**Findings**: `adr-1` (decision), `astro-plan-90u05` (this work)

## What

The schema was stated in three places. `0001_initial_schema.sql` created the
tables, `0002_drop_require_same_camera.sql` removed a column from one of them,
and `0003_plans_destination_root.sql` added a column to another. A reader asking
"what shape is the `plans` table" had to replay the chain, and a reader asking
"may I edit `0001`" got contradictory answers: a compile-time SHA-384 literal in
`crates/persistence/core/tests/baseline_invariants.rs` froze the file's bytes,
while `adr-1` had already decided that pre-1.0 schema changes are made in place.

Nothing consumes the chain. Before 1.0 there is no released database to upgrade,
so every database is created by running the migration set from empty. The two
appended files bought replay fidelity that no installation needs, and paid for it
with three statements of one schema.

## The rule (decision)

**Before 1.0 there is exactly one migration, `0001_initial_schema.sql` at version
1. Schema and seed-data changes are made in place in that file. No `0002` or
later file is added. Every such edit destroys every existing development
database, and recreating it is the user's step.**

The three parts are one rule and none of them stands alone:

| Part | Consequence |
|------|-------------|
| One file at version 1 | the schema is readable in one pass; no replay reconstructs a table's shape |
| Edited in place | `0002+` is not the mechanism for a pre-1.0 change, so the freeze that forbade editing `0001` is removed with the chain |
| Existing dev databases die | an in-place edit changes the recorded checksum of a migration already marked applied, which sqlx refuses at boot |

### The destruction is loud, not silent, and not automatic

`has_pending_migrations` compares applied count against total
(`crates/persistence/core/src/lib.rs:167-186`). An in-place edit to version 1
changes no count, so it reports no pending work and the desktop shell skips its
`VACUUM INTO` backup (`apps/desktop/src-tauri/src/lib.rs:542-554`).
`MIGRATOR.run` then fails on the checksum comparison, classified for the user by
`migration_divergence_detail` (`crates/persistence/core/src/lib.rs:257`) and
surfaced at `apps/desktop/src-tauri/src/lib.rs:631`.

The database is refused, not corrupted, and it is not repairable in place: the
recorded checksum belongs to a schema generation that no longer exists in the
checkout. The reset procedure is
[`docs/development/database-baseline-and-migrations.md`](../../docs/development/database-baseline-and-migrations.md).

Making the backup fire instead would mean teaching `has_pending_migrations` to
compare checksums rather than counts, which is a boot-time read of every
migration against the applied table. That is the 1.0 problem, when a real
upgrade path exists; before 1.0 a developer recreating a database loses nothing
that was not re-derivable.

### Removing the checksum literal leaves no compile-time drift control

The 48-byte SHA-384 literal asserted the migration file's bytes against a
committed constant. It cannot survive a rule that says the file is edited: every
intended edit fails it, so the only possible response is to update the literal in
the same commit, which is a step that proves nothing. A second assertion in the
same test compared `include_str!` of the migration against itself and was
tautological in any case.

Runtime tamper detection is unaffected, because it is sqlx's own comparison of
`_sqlx_migrations.checksum` against the embedded script, not ours. It has a test
(`crates/persistence/core/src/lib.rs:403` `real_sqlx_divergence_is_classified`).

What is genuinely lost is a compile-time signal that the schema changed without
anyone intending it. Replacing it with a snapshot of the schema's *shape* —
tables, columns, indexes — rather than the file's bytes is tracked on
`astro-plan-0zog4` and is out of scope here. Until it lands there is no
compile-time drift control, and `docs/development/database-baseline-and-migrations.md:75-86`
says so.

### Two shipped specs still cite the deleted checksum test

`specs/tiny/documented-fallbacks-on-out-of-range-values.md:98` reasons from
"Migration `0001_initial_schema.sql` is frozen by a checksum test", and
`specs/tiny/bundled-seed-asset-digest.md:70-74` cites that test as the precedent
for its own digest shape. Both are true of the head each was written against and
false now.

Neither is edited. Both are dated, head-pinned records of a decision taken while
the freeze existed, and rewriting their reasoning would misrepresent why the
choice was made. Neither statement drives live behaviour: the first explains why
a read-side guard was chosen over a `CHECK` constraint, and that conclusion is
unchanged; the second borrows a digest-assertion shape that still exists in
`crates/targeting/resolver/tests/bundled_seed_digest.rs`. This section is the
supersession note a reader following either citation needs, and
`astro-plan-ydnh1` tracks adding a forward pointer to each.

## Per-finding verdict, equivalence at base 660d665ec

Equivalence was proven before the two files were deleted, which is the order
`adr-1` requires: a folded baseline that produces a different database than the
chain is a silent schema change, and proving it afterwards proves only that the
folded file matches itself.

Two databases were built, one by running the three-file chain and one by running
the folded `0001`, and compared:

| Check | Verdict | Evidence |
|-------|---------|----------|
| Object count | equivalent | 578 objects in `sqlite_master` on both sides |
| Normalized schema | equivalent | diff of normalized `sqlite_master` SQL is empty |
| Seed data | equivalent | seed `INSERT` statements byte-identical, md5 `9a9f5853b54f321859547e3bfc8645ec` |
| `plans` column list | equivalent | 22 columns both sides, `destination_root` at index 21 |
| Referential integrity | clean | `PRAGMA foreign_key_check` returns no rows |
| `require_same_camera` | absent both sides | removed from the `calibration_tolerances` column list and from the seed row's positional values in one edit |

The seed row and the column list had to change together. The seed `INSERT` is
positional, so dropping the column without dropping its value shifts every
later value by one — a schema-equivalent baseline with wrong seed data, which
the object-count and schema-diff checks would both have passed. The
byte-identical seed comparison is the check that covers it.

## Context

| File | Role |
|------|------|
| `crates/persistence/core/migrations/0001_initial_schema.sql` | Modify: `require_same_camera` column and its positional seed value removed; `destination_root TEXT` added to the `plans` column list |
| `crates/persistence/core/migrations/0002_drop_require_same_camera.sql` | Delete: folded into `0001` |
| `crates/persistence/core/migrations/0003_plans_destination_root.sql` | Delete: folded into `0001` |
| `crates/persistence/core/tests/baseline_invariants.rs` | Modify: SHA-384 literal and tautological `include_str!` comparison removed; test renamed `migration_set_starts_at_baseline_with_unique_versions`; module doc restated |
| `crates/persistence/core/src/schema_cache.rs` | Modify: `MIGRATOR` doc stated append-only and `0002+`; now states the editable baseline |
| `crates/persistence/core/src/lib.rs` | Modify: `migrate` doc referred to a frozen baseline and future append-only files |
| `docs/release/pre-1-0-database-baseline.md` | Modify: baseline migration contract, release reset requirement |
| `docs/development/database-baseline-and-migrations.md` | Modify: single-editable-baseline rule, reset procedure, drift-detection state |
| `docs/development/persistence-layer-hardening.md` | Modify: migrations path corrected; the `0050` latest-migration claim replaced |
| `specs/tiny/editable-pre-1-0-database-baseline.md` | Add: this file |

## Requirements

1. `crates/persistence/core/migrations/` contains exactly one file, at version 1
   with description `initial schema`.
2. The folded baseline produces a database equivalent to the one the chain
   produced, proven before the chain files are deleted.
3. No assertion in the crate fails merely because `0001` was edited as intended.
4. Every statement in the crate's source and in `docs/` that describes migrations
   as frozen or append-only is restated, so no reader is told the opposite of the
   rule.
5. The loss of compile-time drift control is recorded where a reader looking for
   it will be, rather than left to be discovered.

## Tasks

- [x] Prove chain-versus-folded equivalence on object count, normalized schema,
      seed bytes, `plans` column list, and `foreign_key_check`
- [x] Fold `0002` and `0003` into `0001`, removing the `require_same_camera`
      column together with its positional seed value
- [x] Delete `0002_drop_require_same_camera.sql` and
      `0003_plans_destination_root.sql`
- [x] Remove the SHA-384 literal and the tautological `include_str!` comparison
      from `baseline_invariants.rs`; rename the surviving test
- [x] Restate the frozen/append-only language in `schema_cache.rs` and `lib.rs`
- [x] Restate the three affected `docs/` files, including the reset procedure and
      the current absence of compile-time drift control
- [x] Write this spec, carrying the equivalence evidence
- [x] `cargo test -p persistence_core`
- [ ] Schema-shape snapshot replacing the deleted byte checksum — deferred to
      `astro-plan-0zog4`, out of scope by the brief
- [ ] Forward pointers on the two shipped specs that cite the deleted checksum
      test — deferred to `astro-plan-ydnh1`; both are dated records and are not
      rewritten here

## Done When

- [x] One migration file remains, and `migration_set_starts_at_baseline_with_unique_versions`
      asserts version 1 and description `initial schema`
- [x] Equivalence holds on all five checks in the verdict table
- [x] `cargo test -p persistence_core` is green (66 passed, 6 suites)
- [x] No source or documentation statement in scope describes the baseline as
      frozen or the chain as append-only
- [ ] Compile-time drift control exists — it does not; `astro-plan-0zog4` owns
      the replacement and the gap is documented at
      `docs/development/database-baseline-and-migrations.md:75-86`
