# Plan Apply: Research Decisions To Runtime Modules

Maps each research decision in
[`specs/025-filesystem-plan-application/research.md`](../../specs/025-filesystem-plan-application/research.md)
to the code that enforces it. Use this when changing executor behaviour: the
decision text states the invariant, the module is where a regression shows up.

## Decision map

| Decision | Enforced in |
| --- | --- |
| R1 same-volume rename, cross-volume copy-then-delete | `crates/fs/executor/src/ops/move_op.rs` |
| R1 addendum cross-volume failure codes (`copy.succeeded.delete.failed`) | `crates/fs/executor/src/ops/move_op.rs`, `crates/fs/executor/src/failure.rs` |
| R2 archive root routing | `crates/fs/executor/src/ops/archive_op.rs` |
| R2 addendum OS trash per platform | `crates/fs/executor/src/ops/trash_op.rs` |
| R3 failure taxonomy | `crates/fs/executor/src/failure.rs` |
| R4 cancellation between items only | `crates/fs/executor/src/run/loop_.rs`, `crates/app/core/src/plan_apply/terminal.rs` |
| R5 per-item retry | `crates/app/core/src/plan_apply/lifecycle.rs` (`retry_plan_item`) |
| R6 re-apply rejection on non-approved state | `crates/app/core/src/plan_apply/apply.rs` |
| R7 sequential run plus path-set overlap rejection | `crates/app/core/src/plan_apply/paths.rs` (`check_overlap_and_register`, `compute_plan_path_set`) |
| R8 approval-token freshness | `crates/app/core/src/plan_apply/paths.rs` (`verify_approval_token`) |
| R-FS-1 per-item mtime/size CAS | `crates/fs/executor/src/ops/cas_check.rs` |
| R-Pause-1 pause and resume re-validation | `crates/fs/executor/src/run/loop_.rs`, `crates/fs/executor/src/ops/volume_check.rs` |
| R-CAS-1 atomic plan-state CAS on apply start | `crates/persistence/plans/src/repositories/plan_apply.rs` |
| Unclean-shutdown reconciliation | `crates/fs/executor/src/reconcile.rs`, `crates/app/core/src/plan_apply/reconcile.rs` |

## Where runtime differs from the decision text

R8 specifies an HMAC over `(planId, contentHash, approvedAt, serverSecret)`.
The shipped token is a random opaque string,
`format!("tok-{plan_id}-{uuid}")` in `crates/app/core/src/plans/approve.rs`,
compared for equality against the stored column in `verify_approval_token`.
No content hash is recomputed at apply time.

Consequences of the shipped form:

- Single-use and unforgeable-by-guessing hold: the token is a v4 UUID the
  client cannot predict, and re-approval overwrites the stored column.
- The R8 guarantee that an edit between approve and apply is rejected does not
  come from the token. It comes from the per-item CAS in `cas_check.rs`, which
  pauses the run when a source file's current `(mtime, size)` differs from the
  snapshot `plan.approve` recorded.
- A plan-body edit that adds items without touching any approved source file is
  not caught by either mechanism.

## Layering

`crates/fs/executor` has no database, audit-bus, or Tauri dependency. The
executor loop reaches persistence and audit only through `ExecutorCallbacks`
(`crates/fs/executor/src/run/mod.rs`), which `crates/app/core/src/plan_apply/`
supplies. A decision that needs a database read belongs in `app/core`, not in
the executor.
