# Quickstart: Inbox — Drop Parent Items

**Feature**: 058-inbox-drop-parent-items | **Date**: 2026-07-19

How to pick this feature up. Read `spec.md` for *why*, `plan.md` for the three
plan-gate decisions (PG-1/2/3), `data-model.md` for the model and the FR-028
migration, `contracts/operations.md` for the wire surface.

## The one-paragraph version

Scanning a folder writes a placeholder `inbox_items` row with no frame type and
no group key. `classify()` sets that row's state to `classified` anyway, so the
list badge renders a false statement the database itself contains (#711
Instance A). Two read-side patches hid it and traded one regression for another.
This feature deletes the placeholder: scan creates only the source group,
classification creates N real items, and the suppression predicates go away.

## Where the model lives

| Concern | Path |
|---|---|
| Schema | `crates/persistence/core/migrations/0001_initial_schema.sql` — `inbox_source_groups`, `inbox_items`, classification evidence and plan links (originally migrations `0049_inbox_single_type.sql` and `0020_inbox.sql`, since folded into the pre-1.0 baseline) |
| Repository | `crates/persistence/inbox/src/repositories/inbox/` — `items.rs`, `projections.rs`, `source_groups.rs`, `classification.rs`, `metadata.rs`, `plan_links.rs`. The single `inbox.rs` file this table used to cite was split into this module directory, so the upsert/sentinel/purge/sibling-list/suppression line numbers no longer resolve |
| Status-bar count | `crates/persistence/targets/src/repositories/q_desktop.rs` |
| Classify | `crates/app/inbox/src/classify.rs` — result vocabulary `:400-405`, materialization gate `:433`, false state write `:467`, `materialize_sub_items` `:870`, cache seed `:1013` |
| Reclassify | `crates/app/inbox/src/reclassify.rs` — per-item plan block `:81`, #1086 gate `:173-202`, sentinel clear `:218-228`, group-wide interlock `:346-362` |
| Confirm | `crates/app/inbox/src/confirm.rs` — needs-review gate `:174`, TOCTOU `:198`, result gate `:215` |
| Contracts | `crates/contracts/core/src/inbox.rs` — `InboxListItem` `:331`, confirm `:110`, reclassify v2 `:848` |
| Command surface | `apps/desktop/src-tauri/src/commands/inbox.rs` — scan `:295`, placeholder write path `:392-454`, list projection `:514` |
| Desktop UI | `apps/desktop/src/features/inbox/` — `InboxList.tsx` (needs-review read `:63`), `InboxPage.tsx` (detail key `:1160`, `useStaleSelectionCleanup` call `:319`), `InboxDetail.tsx`, `store.ts` |
| Layer-2 journeys | `crates/e2e-tests/tests/inbox_ui_journeys.rs` — helpers `:135`/`:148`, five journeys |

No new crate. The change lives inside `crates/app/inbox` and
`crates/persistence/inbox`, plus one migration and the desktop Inbox feature.

## Running the tests

```bash
just test           # cargo nextest --workspace + doctests + pnpm -r test
just lint           # fmt --check, clippy -D warnings, eslint, pre-commit
just typecheck      # tsc --noEmit
```

Narrower loops while iterating:

```bash
# NOTE: the crate at crates/app/inbox is named `app_core_inbox`, not `app_inbox`.
cargo nextest run -p app_core_inbox     # classify/reclassify/confirm unit + Layer-1
cargo nextest run -p persistence_db     # repository + schema tests (real SQLite, real migrations)
cargo clippy -p app_core_inbox --all-targets -- -D warnings
pnpm --filter @astro-plan/desktop test -- InboxList   # vitest, single suite
```

**Layer-2 Real-UI journeys** are `#[ignore]`d and need `tauri-driver` + a served
frontend; they do not run under `just test`:

```bash
just test-e2e                                    # apps/desktop pnpm test:e2e:real
cargo nextest run -p e2e_tests --run-ignored all # under xvfb + tauri-driver
```

Run the five Inbox journeys locally before pushing anything in phases 2 or 3.
They are the SC-005 gate, they are the surface #1038 broke, and they are not in
the required-checks set — an auto-merge will not wait for them.

## The sequencing trap

`plan.md`'s Implementation phasing is ordered by what unblocks what. One
ordering is not a preference:

> **Stop creating the placeholder BEFORE deleting the suppression predicates.**

The predicates (`exclude_split_placeholder!` and its four call sites) exist only
to hide the placeholder. Delete them while the placeholder is still being
written and every split folder's aggregate row reappears in the list, the stats
and the status-bar badge — which is #1038, the regression that blocked roughly
nine PRs. Deleting them *after* the write path is gone is a no-op on behaviour
and a real deletion of dead code.

The same trap has a second face: `plan.md` phase 1 puts the FR-028 migration
first because it is cheapest under D-004's greenfield licence. If that licence
lapses — any real install — the question reopens (spec Q-1) and the column is no
longer the whole story.

Two more traps worth knowing before you start:

- **PG-1's E2E signal.** `inbox_ui_mixed_folder_splits_into_single_type_items`
  waits on `inbox-mixed-alert` (`inbox_ui_journeys.rs:237`) as its
  proof-of-classify *synchronisation* signal, not as an assertion about
  mixedness. Retiring `mixed` without replacing that wait makes the journey
  **hang for the full timeout rather than fail**, which reads as CI flake. The
  replacement signal is the appearance of the split item rows.
- **#1102 must be decided before phase 3.** `target_recommendations::resolve_item_id`
  (`target_recommendations.rs:227-232`) takes `ids.next()` from a source group's
  items. That is arbitrary today and has no defensible meaning once no row is
  the folder's representative.

## Verification order

1. `cargo nextest run -p persistence_db -p app_inbox` — the model and its
   invariants.
2. `pnpm --filter @astro-plan/desktop test` — the list/detail agreement (FR-008).
3. The five Layer-2 Inbox journeys — SC-005, and the only layer that catches a
   mocked-away crash (the mocks return placeholder-shaped rows with
   `groupKey: ''`, `api/mocks.ts:1744-1793`, and will need updating).
4. Real app on Windows for anything visually validatable — see the
   `verify-on-windows` skill.
