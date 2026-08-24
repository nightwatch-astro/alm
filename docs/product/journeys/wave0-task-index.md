> **MIGRATED:** current truth now lives under `docs/journeys/`, whose
> `INDEX.md` is the generated routing table. This file is frozen legacy
> history from the pre-migration campaign and may not match current
> behavior. See `docs/product/journeys/README.md`.

# Wave 0 — task index (reviewer view)

One row per Wave-0 grilling task (Q15 T120–T127, Q16 T128–T134, Q27 F-Framing-1..11). "Impacted journeys" links the per-journey × task delta file. Tasks with no journey impact carry an indirect-risk note and no delta files. Per-journey rerun unions live in `INDEX.md`.

| Task | Title | Status | Grill | Impacted journeys / note |
|------|-------|--------|-------|--------------------------|
| T120 | Generalize audit entry model + migration (reason_code) | implemented (PR #805, e55b9b36) | Q15 | _none_ — no journey impact — infra/schema. Indirect risk: audit row shape change could regress Audit Log rendering (J13) and lifecycle audit (J5/J7). |
| T121 | Shared audit write-through helper (durable row + bus emit) | implemented (PR #805, e55b9b36) | Q15 | _none_ — no journey impact — infra. Indirect risk: every audited mutation now double-writes; a helper defect would silently drop durable rows (J13 completeness). |
| T122 | Settings mutations write durable audit rows (before→after) | merged-spec | Q15 | [J8](J08-calibration-ingest-masters-matching/deltas/2026-07-14-q15-t122.md), [J10](J10-settings-appearance-i18n/deltas/2026-07-14-q15-t122.md), [J13](J13-audit-activity-investigation/deltas/2026-07-14-q15-t122.md) |
| T123 | Protection overrides/acknowledgements write durable audit rows | merged-spec | Q15 | [J1](J01-first-run-setup-data-sources/deltas/2026-07-14-q15-t123.md), [J6](J06-cleanup-scan-review-apply/deltas/2026-07-14-q15-t123.md), [J7](J07-archive-delete/deltas/2026-07-14-q15-t123.md), [J13](J13-audit-activity-investigation/deltas/2026-07-14-q15-t123.md) |
| T124 | Equipment CRUD writes durable audit rows | merged-spec | Q15 | [J13](J13-audit-activity-investigation/deltas/2026-07-14-q15-t124.md), [J15](J15-equipment-observing-site-setup/deltas/2026-07-14-q15-t124.md) |
| T125 | Source enable/disable/register + rescans/root ops audited | merged-spec | Q15 | [J1](J01-first-run-setup-data-sources/deltas/2026-07-14-q15-t125.md), [J2](J02-ingest-review-reclassify-confirm-move/deltas/2026-07-14-q15-t125.md), [J13](J13-audit-activity-investigation/deltas/2026-07-14-q15-t125.md) |
| T126 | Activity/log panel reads durable audit | blocked-on-Q9 | Q15 | [J10](J10-settings-appearance-i18n/deltas/2026-07-14-q15-t126.md), [J13](J13-audit-activity-investigation/deltas/2026-07-14-q15-t126.md) |
| T127 | Refusal/failure coverage tests + read-negative-space | merged-spec | Q15 | [J12](J12-failure-refusal-handling/deltas/2026-07-14-q15-t127.md), [J13](J13-audit-activity-investigation/deltas/2026-07-14-q15-t127.md) |
| T128 | Absence representable in contract DTOs (Option); guard MasterDetail | merged-spec | Q16 | [J8](J08-calibration-ingest-masters-matching/deltas/2026-07-14-q16-t128.md) |
| T129 | Remove absence-synthesizing fallbacks end-to-end | merged-spec | Q16 | [J4](J04-sessions-review-derived/deltas/2026-07-14-q16-t129.md), [J8](J08-calibration-ingest-masters-matching/deltas/2026-07-14-q16-t129.md) |
| T130 | Shared renderValue renderer + unresolved chip | merged-spec | Q16 | [J12](J12-failure-refusal-handling/deltas/2026-07-14-q16-t130.md) |
| T131 | PropertyTable adopts shared renderer + applicability marker | merged-spec | Q16 | [J2](J02-ingest-review-reclassify-confirm-move/deltas/2026-07-14-q16-t131.md), [J4](J04-sessions-review-derived/deltas/2026-07-14-q16-t131.md), [J8](J08-calibration-ingest-masters-matching/deltas/2026-07-14-q16-t131.md) |
| T132 | Adopt shared renderer across all metadata surfaces | merged-spec | Q16 | [J2](J02-ingest-review-reclassify-confirm-move/deltas/2026-07-14-q16-t132.md), [J4](J04-sessions-review-derived/deltas/2026-07-14-q16-t132.md), [J5](J05-project-lifecycle/deltas/2026-07-14-q16-t132.md), [J7](J07-archive-delete/deltas/2026-07-14-q16-t132.md), [J8](J08-calibration-ingest-masters-matching/deltas/2026-07-14-q16-t132.md), [J9](J09-targets-planning/deltas/2026-07-14-q16-t132.md) |
| T133 | Detail-as-delta rework of every detail panel | merged-spec | Q16 | [J2](J02-ingest-review-reclassify-confirm-move/deltas/2026-07-14-q16-t133.md), [J4](J04-sessions-review-derived/deltas/2026-07-14-q16-t133.md), [J5](J05-project-lifecycle/deltas/2026-07-14-q16-t133.md), [J8](J08-calibration-ingest-masters-matching/deltas/2026-07-14-q16-t133.md), [J9](J09-targets-planning/deltas/2026-07-14-q16-t133.md) |
| T134 | Tests: real 0 / missing / n/a rendering + matching regression | merged-spec | Q16 | _none_ — no journey impact — test task. Verifies the SC-010/SC-011 behavior delivered by T128–T133; rerun those journeys' L1 tests. |
| F-Framing-1 | framing table + join + is_mosaic + session geometry + entity | implemented (e6c23e9e) | Q27 | _none_ — no journey impact — data layer only. Indirect risk: session geometry columns exist but 'populated at confirm' is not yet wired (J2/J3 confirm); legacy rows keep NULL geometry (excluded from clustering). |
| F-Framing-2 | Tolerance-based clustering (suggested framings, R11a) | merged-spec | Q27 | [J5](J05-project-lifecycle/deltas/2026-07-14-q27-f2.md) |
| F-Framing-3 | framing.merge/split/reassign + audit events | merged-spec | Q27 | [J5](J05-project-lifecycle/deltas/2026-07-14-q27-f3.md), [J13](J13-audit-activity-investigation/deltas/2026-07-14-q27-f3.md) |
| F-Framing-4 | Mosaic-mode flag on project create/update | merged-spec | Q27 | [J5](J05-project-lifecycle/deltas/2026-07-14-q27-f4.md), [J14](J14-target-first-project-start/deltas/2026-07-14-q27-f4.md) |
| F-Framing-5 | Inbox-confirm attribution pass (ranked candidates) | merged-spec | Q27 | [J2](J02-ingest-review-reclassify-confirm-move/deltas/2026-07-14-q27-f5.md), [J3](J03-ingest-confirm-catalogue-in-place/deltas/2026-07-14-q27-f5.md) |
| F-Framing-6 | Completed-project attribution → add + reopen | merged-spec | Q27 | [J2](J02-ingest-review-reclassify-confirm-move/deltas/2026-07-14-q27-f6.md), [J5](J05-project-lifecycle/deltas/2026-07-14-q27-f6.md) |
| F-Framing-7 | Per-framing source view + manifest | blocked-on-Q20/Q10 | Q27 | [J5](J05-project-lifecycle/deltas/2026-07-14-q27-f7.md) |
| F-Framing-8 | Layer-1 tests (collapse/merge/split/mosaic/attribution) | merged-spec | Q27 | _none_ — no journey impact — test task. Authors the L1 rerun set for F-Framing-2/3/4/6/10. |
| F-Framing-9 | Quickstart + Windows-E2E scenario + coverage-matrix update | merged-spec | Q27 | _none_ — no journey impact — creates the L2/manual-Windows coverage the framing deltas cite as a gap. Fills the J2/J3/J5 framing coverage gap. |
| F-Framing-10 | Attribution apply-path (chosenAttribution on confirm) | merged-spec | Q27 | [J2](J02-ingest-review-reclassify-confirm-move/deltas/2026-07-14-q27-f10.md), [J3](J03-ingest-confirm-catalogue-in-place/deltas/2026-07-14-q27-f10.md), [J4](J04-sessions-review-derived/deltas/2026-07-14-q27-f10.md) |
| F-Framing-11 | Clustering tunables settings storage + UI | merged-spec | Q27 | [J5](J05-project-lifecycle/deltas/2026-07-14-q27-f11.md), [J10](J10-settings-appearance-i18n/deltas/2026-07-14-q27-f11.md) |
