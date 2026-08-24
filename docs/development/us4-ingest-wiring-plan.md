# US4 ingest grouping — wiring plan (spec 035 gap #2)

**Status:** PLAN ONLY. The resolver seam is built and tested; the blocker is a
missing per-file ingest pipeline, not the resolver.

## What US4 needs (one-line answer)

A production code path that, for each ingested image file, creates a `file_record`
row and calls
`ingest_resolution::associate_or_enqueue(pool, bus, file_record_id, object_raw)`
with the file's FITS `OBJECT` header value. That pipeline does not exist today.

## What already exists (done, do not rebuild)

- `crates/app/core/src/ingest_resolution.rs` — `associate_or_enqueue` (cache-hit
  inline / miss → `pending`) + `resolve_pending` drain. Fully unit-tested. Uses
  gen-3 `canonical_target` only.
- FITS `OBJECT` extraction — `crates/metadata/fits/src/lib.rs` →
  `RawFileMetadata.object` (`crates/metadata/core/src/lib.rs:216`). The value is
  already parsed; today it dead-ends in `crates/app/core/src/inbox/confirm.rs:349-354`
  where it only feeds naming-pattern token resolution (not the resolver).
- `file_record` table — defined in migration `crates/persistence/core/migrations/0001_initial_schema.sql:728`.

## The real gap

`file_record` is **never written by production code** (only tests insert it). Scanning
is folder-level: `crates/fs/inventory/src/lib.rs` + `commands/inbox.rs` create
`inbox_items` (per folder), not per-file rows. So there is no `file_record.id` to pass
as `image_id`, and no place that currently iterates files-with-metadata at ingest time
other than the confirm pipeline (which builds plans, not records).

## Minimal wiring once per-file ingest exists

1. When an image file is ingested/confirmed, create its `file_record` row → get `id`.
2. If `RawFileMetadata.object` is `Some`, call
   `associate_or_enqueue(pool, bus, &file_record_id, object.trim())`.
3. Run `resolve_pending` on a background tick to drain cache-miss enqueues
   (transient/offline stay `pending`, genuine misses → `unresolved`).

Natural hook point: the per-file loop in `inbox/confirm.rs` (it already has the file
path + extracted `meta.object`), **after** file_record creation is added there.

Estimated wiring once the pipeline exists: a few hours. Building the per-file ingest
pipeline itself: ~2–3 days.

## Ownership

The missing per-file ingest (file_record creation + lifecycle transitions) is a
**spec-002 milestone** (spec-002 defines the `file_record` lifecycle states but not the
process that creates/transitions rows). `crates/persistence/core/migrations/0001_initial_schema.sql:29` even notes the
session↔file relational join was "deferred to T006". US4 should be wired as the final
small step of that spec-002 ingest work, or a dedicated ingest spec — not inside spec
035.

## Recommendation

Leave spec-035 US4 as a documented tested seam (current state). Schedule the per-file
ingest pipeline under spec 002 (or a new ingest spec); add the `associate_or_enqueue`
call as an explicit task in that spec so the seam gets connected when the producer of
`file_record.id` lands.
