# PlateVault user journeys

Current journey truth lives under `docs/journeys/`, not in this file and not
under `docs/product/journeys/`:

| For | Read |
|---|---|
| Per-journey routing table (generated; J01–J18) | `docs/journeys/INDEX.md` |
| Journey file format, and how to read one | `docs/journeys/FORMAT.md` |
| Per-project config, cross-cutting product and validator rules | `docs/journeys/README.md` |

`docs/product/journeys/` is the pre-migration catalogue, frozen 2026-07-15. It
is retained as the historical baseline that the live journeys cite by path in
their `trace:` frontmatter, and it holds the 58 Wave 0 behavior deltas that
exist nowhere else. It does not describe current behavior — see
`docs/product/journeys/README.md`.

The J-numbering and each journey's internal stage numbering are canonical: cite
stages by the number and label they carry in
`docs/journeys/JNN-slug/journey.md`, and do not renumber.

## Cross-journey canonical scenarios

The executable, click-by-click counterpart to each journey, under
`e2e-agentic-test/`. Seven journeys have no scenario yet.

| # | Journey | Canonical scenario |
|---|---|---|
| 1 | First-run setup → data sources | `003-first-run-source-setup/wizard-fresh-db-journey` |
| 2 | Ingest → review/reclassify → confirm (move) | `journeys/grand-inbox-journey` |
| 3 | Ingest → confirm (catalogue-in-place) | `journeys/grand-inbox-journey` |
| 4 | Sessions review (derived) | `041-inbox-plan-surface/sessions-derived-inventory` |
| 5 | Project lifecycle create→artifacts | `journeys/full-project-lifecycle` |
| 6 | Cleanup: scan→review→apply | `017-cleanup-archive-review-plans/cleanup-scan-review-apply` |
| 7 | Archive → delete from archive | `017-cleanup-archive-review-plans/archive-lifecycle` |
| 8 | Calibration: ingest→masters→matching | `journeys/calibration-journey-ingest-to-match` |
| 9 | Targets & planning (real vs. stub) | `044-planner-stubs/planner-columns-visibly-stubs` |
| 10 | Settings/appearance/i18n | `018-settings-configuration-model/panes-and-persistence` |
| 11 | Mistake recovery | *(to be authored)* `journeys/mistake-recovery` |
| 12 | Failure & refusal handling | *(to be authored)* `journeys/failure-refusal-handling` |
| 13 | Audit & activity investigation | *(to be authored)* `journeys/audit-investigation` |
| 14 | Target-first project start | *(to be authored)* `journeys/target-first-project` |
| 15 | Equipment & observing-site setup | *(to be authored)* `journeys/equipment-site-setup` |
| 16 | Keyboard-first navigation & windows | *(to be authored)* `journeys/keyboard-first-navigation` |
| 17 | Software update & install | *(to be authored)* `journeys/software-update` |

For execution order, PR-gating, and shared test-data continuity across all of
the above, see `e2e-agentic-test/MASTER-PLAN.md`.
