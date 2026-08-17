# Perf Gate Measurements: Specs 007, 025, 048

**Date:** 2026-07-28
**Status:** Complete
**Harness:** `crates/tools/perf-bench` (scenarios
`calibration_suggest_1k_masters`, `plan_apply_progress`,
`reconcile_root_frames`)
**Related:** spec 007 T033, spec 025 T045, spec 048 T043a/SC-005

---

## 1. What is gated and what is not

`scripts/check-perf-baseline.sh` hard-fails on any increase in a scenario's
`sqlx_stmts` count. It warns without failing when `wall_ms` exceeds 1.5x its
recorded budget. Statement counts are deterministic for a fixed fixture size;
wall time on a shared CI runner is not.

Specs 007 and 025 state their gates as wall-clock thresholds. Those two
thresholds are therefore WARN-only in CI. The enforced budget for all three
scenarios is the statement count.

## 2. Measured results

Measured on Apple silicon under macOS 25.3, release profile. Each scenario ran
three times.

| Scenario | Fixture | sqlx_stmts | wall_ms per run | Spec threshold | Verdict |
|---|---|---|---|---|---|
| `calibration_suggest_1k_masters` | 1,000 masters, 1 light session | 10 | 3, 3, 3 | 200 (007 T033) | met, 66x margin |
| `plan_apply_progress` | 10,000-item plan | 40,113 | 42, 43, 71 (progress gap) | 50 (025 T045) | missed in 1 of 3 runs |
| `reconcile_root_frames` | 10,000 present frames | 9 | 8040, 7660, 7776 | none stated (048 SC-005) | see §4 |

`sqlx_stmts` was identical across all three runs for every scenario.

## 3. Spec 025 T045: the progress-gap threshold

T045 measures the interval between consecutive operation events reaching the
sink, reported as `max_progress_gap_ms`. Progress is emitted one envelope per
group-commit flush window, bounded by `FLUSH_ITEM_COUNT = 100` items or
`FLUSH_INTERVAL = 250` milliseconds
(`crates/app/core/src/plan_apply/callbacks.rs`). A 10,000-item plan produced
100 progress events plus start and completion.

The gap therefore tracks how long 100 items take to execute and flush, not
per-item latency. The slowest of three runs exceeded T045's threshold (see the
table in §2). Because wall time is WARN-only, CI does not fail on this; the
miss is recorded here rather than absorbed into the baseline.

Lowering `FLUSH_ITEM_COUNT` would tighten the gap at the cost of more
transactions per run. That tradeoff is not made here.

## 4. Spec 048 SC-005: reconcile scaling is superlinear

SC-005 requires a 10,000-frame reconcile to complete "without blocking the UI
and reports progress throughout". The scenario measures the first half only.
Incremental `progress_pct` streaming is documented as a future long-running
operation extension in `crates/contracts/core/src/inventory_frame.rs:110-112`,
so the second half is not measurable against the current contract.

The pass issues 9 SQL statements for 10,000 frames, so the database path is
batched. Wall time is dominated by the path-matching walk:

| Frames | wall_ms |
|---|---|
| 2,500 | 662 |
| 10,000 | 7,776 |

Four times the frames costs twelve times the wall time. `reconcile_root` at
`crates/fs/inventory/src/reconcile.rs:121` scans the full disk-file vector once
per known frame, giving O(frames x files). A hash-set lookup would make the
pass linear.
