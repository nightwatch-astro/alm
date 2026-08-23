# Audit Log Pruning

**Status**: Design accepted, unimplemented
**Bead**: astro-plan-zojet

Retention design for the audit and event tables. Covers the prunable and exempt
row classes, projected log size, and the settings that control retention.

## Summary

- `audit_log_entry`, `plan_apply_events`, and `events` each receive one row per
  plan item applied
  (`crates/app/core/src/plan_apply/callbacks.rs:184`, `:215`, `:223`).
  A plan item is one file operation, so this class scales with library size.
- Every other audit producer writes one row per user command. There are 17 such
  sites (census below), and together they account for single-digit MB per decade.
- The per-item class exceeds the per-command class by roughly 96x per year on a
  400,000-frame library. Retention must therefore be scoped by class, not
  applied uniformly by age.
- A per-item row is prunable only where `plan_items` records the same fact in a
  dedicated column. `plan_items` carries the intent (`action`, paths), the
  outcome (`item_state`, `failure_reason`), and the destructive-operation
  destination (`archive_path`). One of the seven per-item audit triggers records
  state that `plan_items` cannot express, so it is Tier 1. `plan_items`,
  `plans`, and `plan_apply_runs` are Tier 1 and are exempt.
- The design specifies prunable triggers as a closed allowlist of exact strings,
  not a `LIKE` prefix. A trigger absent from the allowlist is retained, so a new
  trigger is exempt until someone adds it and states the derivability argument.
- Multi-year retention is feasible. Ten years of per-command audit costs about
  25 MB. Ten years of per-item audit on the largest modelled library costs about
  3.0 GB, which the 730-day default reduces to about 0.6 GB.

## What already exists

| Component | Location | State |
| --- | --- | --- |
| Age-based `events` pruner, 90-day constant | `crates/audit/src/pruner.rs:35`, `:46`, `:60` | No caller anywhere in the repo or app shell |
| `DELETE FROM events WHERE emitted_at < ?` | `crates/persistence/lifecycle/src/repositories/events.rs:310` | `pub`, reached only from `pruner::spawn`; filters on `emitted_at` alone |
| Retention-gap marker for the log view | `crates/app/core/src/log_stream.rs:228` | Shipped; reports `truncated` when a cursor predates the oldest row |
| Time-range log export | `crates/app/core/src/log_stream.rs:276` | Shipped; covers `events` only |
| `VACUUM INTO` snapshot copy | `crates/persistence/core/src/lib.rs:188` | Shipped for backup, not for space reclamation |

`pruner::spawn` is the only pruning code in the tree and nothing invokes it, so
no table is pruned at runtime today. The pool sets no `auto_vacuum` and no
`page_size` pragma (`crates/persistence/core/src/lib.rs:119`), so the SQLite
defaults apply: a `DELETE` returns pages to the free list and the database file
does not shrink.

The shipped predicate at `crates/persistence/lifecycle/src/repositories/events.rs:310`
filters on `emitted_at` alone. It carries neither a terminal-plan guard nor a
`topic` restriction, so a caller wired to it deletes `plan.item.progress` rows
for a plan still applying. That is inside Tier 2, because `reconcile.rs:20`
never reads `events`, and it still contradicts the terminal-state guard this
design requires of every prune predicate. Adding the guard is a precondition on
wiring `pruner::spawn`, tracked as its own work item below.

## Producer census

| Producer | Table | Cardinality | Site |
| --- | --- | --- | --- |
| Plan-apply group commit | `plan_apply_events` | 1 per plan item | `crates/app/core/src/plan_apply/callbacks.rs:184` |
| Plan-apply group commit | `audit_log_entry` | 1 per plan item | `crates/app/core/src/plan_apply/callbacks.rs:215` |
| Plan-apply group commit | `events` (`plan.item.progress`) | 1 per plan item | `crates/app/core/src/plan_apply/callbacks.rs:223` |
| Plan-apply divergence marker, `trigger` `plan_item.persist_divergence` | `audit_log_entry` | 1 per item, only on double flush failure | `crates/app/core/src/plan_apply/callbacks.rs:390` |
| Plan-apply terminal path, `trigger` `plan.bulk_cancel_degraded` | `audit_log_entry` | 1 per run | `crates/app/core/src/plan_apply/terminal.rs:458` |
| Settings writes, restore-defaults, source overrides | `audit_log_entry` | 1 per command | `crates/app/settings/src/writes.rs:164`, `:223`, `:448`, `:478` |
| First-run root registration and remap | `audit_log_entry` | 1 per command | `crates/app/core/src/first_run/mod.rs:246`, `:298`, `root_ops.rs:70`, `:185`, `root_remap.rs:230` |
| Source and plan protection checks | `audit_log_entry` | 1 per command | `crates/app/core/src/protection/source_protection.rs:142`, `plan_check.rs:148` |
| Calibration equipment and assignment | `audit_log_entry` | 1 per command | `crates/app/calibration/src/equipment.rs:118`, `matching/assign.rs:48` |
| Inbox attribution | `audit_log_entry` | 1 per project attribution | `crates/app/inbox/src/attribution.rs:753` |
| Framing selection | `audit_log_entry` | 1 per command | `crates/app/core/src/framing.rs:97` |
| Lifecycle transition | `events` (`lifecycle.transition.applied`) | 1 per asset transition | `crates/persistence/lifecycle/src/repositories/lifecycle.rs:529` |
| Artifact observation | `events` | 1 per detected artifact file | `crates/app/lifecycle/src/artifact/detect.rs:53`, `classify.rs:57`, `reconcile_batch.rs:308` |
| Command ledger | `command_execution`, `outbox_event`, `audit_event`, `repository_change` | 1 per command, plus 1 outbox row per emitted event | `crates/persistence/lifecycle/src/repositories/command_ledger/mod.rs`, `finish.rs` |
| Provenance supersession | `provenance_history_archive` | 1 per superseded field value | `crates/persistence/lifecycle/src/repositories/provenance.rs:42` |

Artifact observation is per file but bounded by processing output rather than
library size: one row per stacked or edited output, not one per captured frame.

## Tier boundary

The durability test comes from Principle V. A row is Tier 2 when the application
can re-derive it from the filesystem or from another table, and Tier 1 when its
loss destroys user knowledge.

| Table | Tier | Prunable | Reason |
| --- | --- | --- | --- |
| `plans` | 1 | No | Boot reconciliation reads `plans.state` as the mutation intent (`crates/app/core/src/plan_apply/reconcile.rs:20`). `plan_items` has `ON DELETE CASCADE` on this row (schema line 1732), so deleting a plan destroys its items. |
| `plan_items` | 1 | No | Records a destructive mutation and nothing else does: `action` in (`delete`, `archive`), `archive_path`, `item_state`, `failure_reason`, `approved_mtime`, `approved_size_bytes` (schema lines 1735 to 1757). A deleted file cannot be re-derived from the filesystem. |
| `plan_apply_runs` | 1 | No | Boot reconciliation reads it alongside `plans.state` (`reconcile.rs:21`). |
| `provenance_history_archive` | 1 | No | `origin` values `reviewed` and `applied` encode user decisions, and inline history is capped at 10 entries per field (`provenance.rs:42`), so the archive is the only full record. |
| `outbox_event` where `published_at IS NULL` | 1 | No | An unpublished row is undelivered work, which `idx_outbox_event_unpublished` indexes. |
| `audit_log_entry` where `trigger` is outside the prunable allowlist | 1 | No | The fail-closed default. It covers the command triggers in the census above, `plan_item.persist_divergence`, and any trigger added later. `settings.restore_defaults` (`crates/app/settings/src/writes.rs:359`) carries the pre-restore snapshot in its payload only. For a command trigger, `outcome` values `refused` and `failed` record a rejected request that appears nowhere else. |
| `audit_log_entry` where `trigger` is in the prunable allowlist | 2 | Yes | Each of the six listed triggers restates an `item_state`, `item_stale`, or `failure_reason` value that `plan_items` holds in its own column. The transition timestamp is the only unique content and it is history, not a user decision. |
| `plan_apply_events` | 2 | Yes | Per-item transition timeline for a run whose outcome is in `plan_items.item_state`. Not read by boot reconciliation. |
| `events` | 2 | Yes | Live and replay feed for the log view. Hooks are idempotent, so a subscriber replaying past a pruned cutoff re-dispatches no-ops (`crates/audit/src/pruner.rs:20`). |
| `audit_event`, `outbox_event` where `published_at IS NOT NULL`, `command_execution`, `repository_change` | 2 | Out of scope | Per-command volume, foreign-keyed into each other and into `audit_event.created_sequence`. Pruning them needs its own dependency-order design and is not justified by their size. |

The boundary is a `trigger` predicate on `audit_log_entry`, not the `severity`
column. The per-item rows are written at `Severity::Workflow`
(`crates/app/core/src/plan_apply/callbacks.rs:472`), so severity does not
separate the high-volume class from the rest.

### Prunable trigger allowlist

`callbacks.rs:393`, `:469`, and `:568` are the sites that write a `plan_item.`
trigger, and `:469` interpolates the executor's terminal state, so the prefix
`plan_item.` expands to the seven trigger strings below. The "Recoverable from"
column is the check a reader runs: open the cited column and confirm the audit
row states nothing the column does not.

| Trigger | Written at | Recoverable from | Tier | Prunable |
| --- | --- | --- | --- | --- |
| `plan_item.succeeded` | `callbacks.rs:469` from `crates/fs/executor/src/run/loop_.rs:512` | `plan_items.item_state` = `succeeded` | 2 | Yes |
| `plan_item.failed` | `callbacks.rs:469` from `loop_.rs:472`, `:531` | `plan_items.item_state` = `failed` plus `failure_reason` | 2 | Yes |
| `plan_item.skipped` | `callbacks.rs:469` from `loop_.rs:94` | `plan_items.item_state` = `skipped` | 2 | Yes |
| `plan_item.stale` | `callbacks.rs:469` from `loop_.rs:445` | `plan_items.item_state` = `failed` plus `item_stale` = 1, set by `crates/persistence/plans/src/repositories/plan_apply.rs:740` | 2 | Yes |
| `plan_item.cancelled` | `callbacks.rs:568` | `plan_items.item_state` = `cancelled`, written by `batch_cancel_pending_items` before the audit row (`crates/app/core/src/plan_apply/lifecycle.rs:131`) | 2 | Yes |
| `plan_item.refused` | `callbacks.rs:469` from `loop_.rs:393`, `:414` | `plan_items.item_state` = `failed` plus the `FailureCode` prefix of `failure_reason` | 2 | Yes |
| `plan_item.persist_divergence` | `callbacks.rs:393` | Nothing | 1 | No |

`plan_item.refused` is Tier 2, and the two paths that produce it are the
destructive-confirmation gate at `loop_.rs:393`, `reason_code`
`destructive_unconfirmed`, and the path gate at `loop_.rs:414`, whose
`reason_code` is the `FailureCode` string. The protection check at `loop_.rs:472`
is not one of them: it emits `new_state` `failed`, so it produces
`plan_item.failed`.

`refused` has no `item_state` value of its own, because `plan_apply.rs:735`
collapses `refused` and `stale` to `failed`. The reason survives in
`plan_items.failure_reason` rather than only in the audit row.
`callbacks.rs:513` stores `PlanItemFailure` through `Display`, which
`crates/fs/executor/src/failure.rs:50` formats as the code, a colon, then the
message, so the column value begins with a stable `FailureCode` string, and
`plan_apply.rs:742` writes that column on the same `UPDATE` as `item_state`.
Reading the code is a prefix split on the first colon, not message parsing.

The row is prunable under the Principle V test on two grounds. A refusal returns
before `execute_item`, so the filesystem was never mutated and no custody fact
can be lost. The intent, the paths, the outcome, and the reason code remain in
`plan_items` permanently.

The residue is the `refused` label itself:

- `root_escape` and `symlink` (`crates/fs/executor/src/ops/path_gate.rs:67`,
  `:98`) and `destructive_unconfirmed` appear on no other terminal path, so the
  label follows from the code.
- `path.invalid` is also produced on the execution path by `failure.rs:187`,
  `crates/fs/executor/src/ops/write_manifest_op.rs:30`, and
  `crates/fs/executor/src/run/dispatch.rs:119`, which record `item_state`
  `failed`. Pruning the audit row for such an item drops whether the gate
  refused it or the operation failed on it.

Neither branch mutated the filesystem, so that one distinction is diagnostic
history rather than user knowledge, which is what puts the row in Tier 2.

The rejected alternative is exempting `plan_item.refused` as Tier 1 by analogy
with the command triggers whose `outcome` is `refused`. The analogy does not
hold. A refused command does not write a row anywhere else, whereas a refused
plan item leaves a `plan_items` row with its action, its paths, and its reason
code.
Exempting it would also cost the retention policy its point: refusals rise with
protected sources and unconfirmed deletes on exactly the large libraries whose
per-item volume motivates pruning.

`plan_item.persist_divergence` is Tier 1 by construction.
`crates/app/core/src/plan_apply/callbacks.rs:240` writes it only after the
`plan_items` flush and its single retry have both failed, and `:242` labels that
path TIER-1 durability. The row exists because the `plan_items` write did not
land, so the table it would be derived from is the table that is missing the
data. `callbacks.rs:385` names the crash-recovery sweep as its reader, and
`crates/app/core/src/plan_apply/reconcile.rs:20` reads `plans.state`, the
`plan_apply_runs` row, and `plan_items.item_state` only, so no shipped consumer
reads the marker. Pruning it removes the input before its reader exists.

The allowlist belongs in `crates/audit-types/src/event.rs` as a constant, beside
the `AuditLogEntry` type it classifies (`:141`). That file is the placement the
crate graph forces: `crates/audit` holds the pruner and `crates/app/core` holds
the write sites, and both depend on `audit-types`, so one constant serves the
predicate and the author adding a trigger edits the file that defines it. No such
constant exists in the tree, and the work item below tracks adding it.

These requirements on the implementation make the enumeration fail closed:

1. The prune predicate binds `trigger IN (...)` from the constant. `LIKE` is
   rejected: a prefix match is an allowlist written as a wildcard, so it admits
   every future `plan_item.` trigger without review.
2. `callbacks.rs:469` interpolates `event.new_state`, a `String` the executor
   supplies, so a new terminal state yields a new trigger with no edit at the
   write site. Under `IN`, that trigger is retained.
3. A test must assert that every allowlist entry is `plan_item.<state>` where
   `<state>` is in the `plan_items.item_state` CHECK set
   (`crates/persistence/core/migrations/0001_initial_schema.sql:1748`), except
   for `stale` and `refused`, which `plan_apply.rs:735` collapses to `failed` and
   which the two arguments above cover. An entry `plan_items` cannot express
   fails the test.

## Row sizes

Computed from the schema and the payload constructors, not measured against a
running database. Sizes are the SQLite record bytes plus the index entries each
row creates, at the default 4096-byte page size.

| Table | Record | Index entries | Total |
| --- | --- | --- | --- |
| `audit_log_entry` | 410 B | 169 B across 5 indexes | 579 B |
| `plan_apply_events` | 209 B | 191 B across 3 indexes | 400 B |
| `events` | 358 B | 28 B on `idx_events_topic` | 386 B |
| Per plan item | | | 1365 B |

Assumptions behind the record sizes, each disputable on its own:

- Identifier columns hold 36-byte UUID text. `audit_log_entry` carries four
  (`audit_id`, `entity_id`, `request_id`, and the `planId` in its payload);
  `plan_apply_events` carries four (`id`, `run_id`, `plan_id`, `item_id`).
- Timestamps are RFC-3339 UTC at 30 bytes.
- `audit_log_entry.payload` is the five-key object at
  `crates/app/core/src/plan_apply/callbacks.rs:476`, about 184 bytes with null
  failure fields.
- `events.payload` is the nine-field `PlanItemProgress` object at
  `callbacks.rs:442`, about 294 bytes with null failure fields.
- A successful item leaves `reason_code`, `failure_code`, `failure_message`, and
  the three rollback columns null. A failing item adds its failure message to
  all three tables, so a run with many failures costs more per item.
- Index entry sizes count the key, the rowid varint, the record header, and the
  cell pointer. `TEXT PRIMARY KEY` on a rowid table creates an implicit unique
  index, counted for `audit_log_entry` and `plan_apply_events` and not for
  `events`, whose `INTEGER PRIMARY KEY AUTOINCREMENT` is the rowid.

Page slack adds to the on-disk figure. The projections below use 1.5 KB per plan
item, which is the 1365-byte computed total plus 10 percent.

## Projected growth

Library sizes are modelled as frame counts. The largest case, 400,000 frames at
about 30 MB per frame, is roughly 12 TB of image data.

First organize, one plan item per file:

| Library | Frames | Plan items | Rows | Bytes |
| --- | --- | --- | --- | --- |
| Small | 20,000 | 20,000 | 60,000 | 30 MB |
| Medium | 100,000 | 100,000 | 300,000 | 150 MB |
| Large | 400,000 | 400,000 | 1,200,000 | 600 MB |

Steady state per year, assuming new captures plus one cleanup or archive pass
over 25 percent of the library:

| Library | New frames | Cleanup items | Plan items | Per-item bytes | Per-command bytes |
| --- | --- | --- | --- | --- | --- |
| Small | 5,000 | 5,000 | 10,000 | 15 MB | 2.5 MB |
| Medium | 20,000 | 25,000 | 45,000 | 68 MB | 2.5 MB |
| Large | 60,000 | 100,000 | 160,000 | 240 MB | 2.5 MB |

Assumptions for the yearly rows:

- New frames per year are one quarter of the library for the small case and one
  fifth for the medium and large cases.
- One cleanup or archive pass per year over 25 percent of the library. A user who
  does not run a cleanup pass pays only the new-frame column; a user who
  reorganises the whole library twice pays about eight times the large-case
  figure.
- Per-command bytes assume 5,000 audited commands per year at about 500 bytes
  each. This column does not vary with library size because command count tracks
  user activity, not frame count.
- Retries and failures are excluded. Each retried item adds one more row to each
  of the three tables.

Ten-year totals for the large library: 600 MB of first organize, 2.4 GB of
steady state, 25 MB of per-command audit, for about 3.0 GB. Under the 730-day
default the per-item component holds at about 0.6 GB.

## Feasibility

Retention of years rather than months is feasible, with one class as the
constraint.

- Per-command classes: indefinite retention costs 25 MB per decade. No parameter
  is warranted, so the design retains them forever.
- Per-item class: 240 MB per year at the large size, against 2.5 MB for
  everything else. It is the only class that scales with library size, and it
  dominates by about 96x.
- Uniform indefinite retention puts a 12 TB library at about 3.0 GB of log after
  a decade. That is 0.025 percent of the image data it describes, and a database
  large enough to slow the log view and every backup `VACUUM INTO`.

One class dominating by that factor is what makes the design prune by class
rather than uniformly by age.

The 730-day default costs about 480 MB at the large size and keeps two years of
per-item transition history. A user who lowers it to 90 days loses the transition
timeline of completed plans. The record of what was moved, archived, or deleted
survives, because `plan_items` retains the intent and the outcome permanently.

## Parameters

Both parameters are stable settings keys registered in
`crates/app/settings/src/descriptors.rs:93`, hydrated into `SettingsState` by
their `apply` closure and read from that applied in-memory value. Persistence to
the `settings` table may lag, per the write-behind rule in Principle V. Neither
is `overridable`, because retention is a database-wide property and not a
per-source-root one. Both are `noisy: false`, so a change writes one
`settings.update` audit row rather than a snapshot.

| Key | Type | Unit | Default | Validation |
| --- | --- | --- | --- | --- |
| `auditRetentionDays` | integer | days | 730 | `NumberRangeInclusive`, 0 to 36500 |
| `eventLogRetentionDays` | integer | days | 90 | `NumberRangeInclusive`, 0 to 36500 |

`ValidationRule::NumberRangeInclusive` takes four fields, `lo: f64`, `hi: f64`,
`msg`, and `want_msg` (`crates/app/settings/src/descriptors.rs:44`), so the
bounds are `f64` while the stored value is a whole number of days. Both keys
therefore need the two message strings alongside the range.

`auditRetentionDays` governs `audit_log_entry` rows whose `trigger` is in the
prunable allowlist and all `plan_apply_events` rows, in both cases only for
plans in a terminal state. `0` disables pruning of those classes. An absent
`settings` row resolves to the descriptor default, not to 0: `get_raw` returns
`None` (`crates/persistence/lifecycle/src/repositories/settings.rs:37`),
`apply_value_to_state` is never called for the key
(`crates/app/settings/src/read.rs:111`), and the `SettingsState` field keeps its
`Default` value, which `default_value_for_key` reports (`read.rs:120`). So unset
means 730 and the implementation must set that field default to 730.

- Lower it to 90 when the database has grown past a size the user finds
  acceptable on an SSD, or before handing the library to another machine.
- Raise it to 3650 when the user wants the full per-item timeline for a decade
  and has the 3 GB.
- Set it to 0 when the user wants no automatic deletion at all.

`eventLogRetentionDays` governs the `events` table. The 90-day default matches
the shipped `DEFAULT_RETENTION_DAYS` (`crates/audit/src/pruner.rs:35`). An
absent row resolves to 90 by the same descriptor path.

- Lower it to 7 when the log view is only used for the current session and the
  database should stay small.
- Raise it to 365 when the user exports logs for support and wants a year of
  history available to `log.export`.
- Set it to 0 to disable, at which point the table grows without bound.

The keys are separate because the tables serve different purposes. `events` is
the live feed for the log view, and its rows are the most redundant of the three.
`audit_log_entry` is the history a user reads to answer what happened to a file.
Using one key for both forces the same window on both.

### Trigger and schedule

Pruning is age-triggered and automatic: one pass at startup, then one per 24
hours, which is the shape `crates/audit/src/pruner.rs:60` already implements. A
manual `log.prune` command exists in addition, for a user who lowers retention
and wants the effect immediately rather than at the next startup.

`log.prune` takes one argument, `reclaimSpace: bool`, default false. With it
set, the pass runs `VACUUM` after deleting. Without it, the freed pages stay on
the free list and the database file does not shrink, because the pool sets no
`auto_vacuum` (`crates/persistence/core/src/lib.rs:119`). A user pruning to
recover disk space needs the vacuum; a user pruning to speed up the log view
does not, and pays neither the full-file rewrite nor the transient doubling of
disk use.

### Rejected parameters

| Rejected | Reason |
| --- | --- |
| Row-count cap | A user cannot map a row count to anything they care about. Rows per file operation is an implementation detail. |
| Database-size cap | The file does not shrink on delete, so a size-triggered pass would fire repeatedly without changing the measured size unless it also vacuumed. Coupling a size trigger to a full-file rewrite is a worse default than an age trigger. |
| Automatic or on-request mode enum | `auditRetentionDays = 0` already expresses "never delete automatically", and `log.prune` already covers "delete now". |
| Per-table retention keys beyond the two above | The remaining tables are Tier 1 or out of scope, so there is no window to configure. |
| Vacuum-after-prune setting | The choice depends on the reason for the individual prune, not on a standing preference, so it belongs on the `log.prune` call. |

## Constitutional constraints

**Principle II, archive or trash over permanent deletion.** The preference
applies to files and not to audit rows. A file is user-owned source material that
cannot be reacquired, so deleting it is irreversible loss. A pruned audit row is
a projection whose underlying records survive it: `plan_items` holds the intent,
the outcome, and the destructive destination of every mutation, and the
attribution and lifecycle decisions stay in their own tables.

The equivalent of an archive is the export path at
`crates/app/core/src/log_stream.rs:276`, which writes a time range to a file the
user chooses. That export covers `events` only. Extending it to
`audit_log_entry` is therefore a precondition for shipping the audit prune.

**Principle II, reviewable plan before mutation.** The requirement is scoped to
filesystem mutation and a prune touches only the database, so it does not apply.
The user-facing review surface is:

1. A preview on `log.prune`: rows to delete per table and the cutoff timestamp,
   returned before any delete runs.
2. The retention-gap marker already shipped at
   `crates/app/core/src/log_stream.rs:228`, which reports `truncated` when a
   caller's cursor predates the oldest surviving row, so a pruned window is
   visible in the log view rather than silent.
3. The prune's own audit row, below.

**Prune traceability.** Each pass writes one `audit_log_entry` with
`trigger` `audit.pruned`, `severity` `workflow`, `actor` `system`, and a payload
carrying the cutoff, the per-table deleted counts, and the oldest and newest
`at` values removed. Its `trigger` is absent from the prunable allowlist, so the
predicate exempts it and the prune history is never pruned. One row per day is
about 0.2 MB per year.

**Unclean-shutdown reconciliation.** Pruning cannot remove a row that boot
reconciliation needs. `crates/app/core/src/plan_apply/reconcile.rs:20` states
the intent is `plans.state` plus the `plan_apply_runs` row and the outcome is
`plan_items.item_state`; the pass reads only those. Independent guards hold
this:

1. All three tables the pass reads are Tier 1 and no prune predicate targets
   them.
2. Every prune predicate additionally requires the plan to be in a terminal
   state, so an interrupted mutation is out of scope for pruning regardless of
   its age.
3. `plan_item.persist_divergence` is outside the prunable allowlist. The
   terminal-state guard alone does not protect it, because a run whose flush
   diverged still finalizes terminal, so the exemption has to be in the trigger
   enumeration rather than in the state predicate.

The `ON DELETE CASCADE` from `plans` to `plan_items` (schema line 1732) is the
failure mode to guard against in future work: any feature that deletes a plan
row destroys the only durable record of what its items did to the filesystem.

## Implementation work

Filed as beads from this node.

| Work | Depends on |
| --- | --- |
| Wire `pruner::spawn` into the app shell and drive it from `eventLogRetentionDays` | Settings descriptors |
| Register `auditRetentionDays` and `eventLogRetentionDays` descriptors | |
| Prunable-trigger allowlist constant in `crates/audit-types/src/event.rs`, with the CHECK-set test from the allowlist section | |
| Class-scoped prune for `audit_log_entry` and `plan_apply_events`, with the terminal-plan predicate, `trigger IN` against the allowlist, an exemption test for `plan_item.persist_divergence`, and a test that an unlisted `plan_item.` trigger is retained | Settings descriptors, allowlist constant |
| Index supporting the `trigger IN` predicate on `audit_log_entry` (migration) | Class-scoped prune |
| Terminal-plan guard for the `events` pruner before `pruner::spawn` gains a caller | Allowlist constant |
| `audit.pruned` audit row per pass | Class-scoped prune |
| `log.prune` command with preview and `reclaimSpace` | Class-scoped prune |
| Extend `log.export` to `audit_log_entry` | |
