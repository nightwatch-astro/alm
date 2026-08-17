# Feature Specification: First-run wizard scan step

**Feature Branch**: `038-wizard-scan-step`

**Created**: 2026-06-19

**Status**: Implemented (post-hoc record, verified 2026-07-28). The wizard scan
step ships in `apps/desktop/src/features/setup/SetupWizard.tsx` (`SCAN_STEP` /
`isOnScanStep`, around lines 555-609). This header claimed otherwise until the
2026-07-28 spec audit corrected it.

**Input**: As the last step in the setup wizard, perform the actual scan per registered source and show the status of the scan (what was detected per source). Ingestion-group approval for brownfield libraries happens in the Inbox afterward — the wizard step is scan + summary only and reuses the Inbox triage components for the summary view.

## Background

Today the wizard ends at a static **Confirm** step that says "an initial scan runs after
setup," then registers roots and navigates to the Inbox. Users get no feedback that anything was
detected. This feature replaces/augments that with an actual scan + per-source detection summary,
so first-run users immediately see what the app found in their library before they start
triaging in the Inbox.

**Pipeline reality** (grounding): `scan_start` is a stub. The real ingestion path is the Inbox
pipeline — `inbox_scan_folder` (scan a folder) → `inbox_classify` (group/classify detections) —
with `inbox_confirm` performing per-group approval. `crates/app/core/src/inbox/confirm.rs` is
real and tested. This feature drives the existing **scan + classify** commands from the wizard;
**confirm/approval stays in the Inbox** (decided).

## Clarifications

- **Approval model**: Scan + summary only. The wizard runs scan + classify per source and shows
  what was detected; the user approves ingestion groups later in the Inbox (Finish is never
  blocked on approval).
- **UI**: Reuse the existing Inbox triage components for the per-source detection summary rather
  than building a parallel UI.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See what was detected per source (Priority: P1)

After registering sources, the user reaches the final wizard step which scans each source and
shows, per source, the detected ingestion groups (counts, types) so they understand what the app
found before entering the Inbox.

**Why this priority**: This is the feature's core value — turning a blind "scan happens later"
into visible, trustworthy first-run feedback.

**Independent Test**: With mock data, complete the wizard to the scan step; each registered
source shows a scanning→done progression and a summary of detected groups.

**Acceptance Scenarios**:

1. **Given** registered sources, **When** the scan step opens, **Then** each source shows scan
   progress and, on completion, a summary of detected ingestion groups (reusing Inbox triage
   components).
2. **Given** a source with no recognizable files, **When** its scan completes, **Then** it shows
   an empty/"nothing detected" state without error.
3. **Given** a scan error for one source, **When** it fails, **Then** that source shows an error
   state and the others still complete (one failure doesn't abort the step).

### User Story 2 - Finish into the Inbox to approve (Priority: P1)

From the scan summary, the user completes setup and lands in the Inbox where the detected groups
await approval (`inbox_confirm`), exactly as if they had scanned from the Inbox.

**Why this priority**: Approval is deliberately kept in the Inbox; the wizard must hand off
cleanly so detections are actionable.

**Independent Test**: Finish from the scan step → Inbox shows the same detected groups ready to
confirm.

**Acceptance Scenarios**:

1. **Given** completed scans, **When** the user clicks Finish, **Then** setup completes and the
   app navigates to the Inbox showing the detected groups.
2. **Given** the user wants to skip review, **When** they Finish, **Then** nothing is
   auto-approved — groups remain pending in the Inbox.

### Edge Cases

- Large source: show progress and keep the UI responsive; do not block other sources.
- Re-entering the step (back/forward) should not double-scan or duplicate detections.
- Mock mode must drive the same step against mock scan/classify responses.

## Requirements *(mandatory)*

- **FR-001**: The final wizard step MUST scan each registered source using the Inbox scan command
  and classify detections using the Inbox classify command.
- **FR-002**: The step MUST show per-source progress (pending → scanning → done/error) and, on
  completion, a summary of detected ingestion groups per source.
- **FR-003**: The summary MUST reuse the existing Inbox triage components (no parallel UI).
- **FR-004**: The step MUST NOT perform ingestion-group approval; approval happens in the Inbox
  via the existing confirm flow.
- **FR-005**: A scan failure for one source MUST NOT abort the step or block Finish.
- **FR-006**: Finishing MUST complete first-run and navigate to the Inbox with detections pending.
- **FR-007**: The step MUST function in mock mode against mock scan/classify responses.
- **FR-008**: No ingestion is auto-approved by the wizard.

## Success Criteria *(mandatory)*

- **SC-001**: From a fresh first-run with N sources, the scan step shows N per-source results and
  a non-zero detection summary for sources containing recognizable files.
- **SC-002**: Finishing lands in the Inbox with the same detected groups pending approval.
- **SC-003**: A simulated single-source scan failure leaves the other sources completed and
  Finish enabled.
- **SC-004**: Full vitest suite (mock-backed) covers the step's states (scanning, done, empty,
  error) and passes.

## Scope

**In scope**: a wizard scan step driving inbox scan + classify per source; per-source progress +
detection summary reusing Inbox components; clean handoff to the Inbox on Finish.

**Out of scope**: in-wizard group approval/confirm (stays in Inbox); wiring the stubbed
`scan_start` background scanner; spec-029 sessions persistence; any change to classification rules.

## Assumptions

- `inbox_scan_folder` + `inbox_classify` accept a source path/root and return detections suitable
  for the Inbox triage components.
- Detections persist (DB) so they are visible in the Inbox after navigation.
- Greenfield: replacing the static Confirm step's "scan after setup" copy is acceptable.

## Accepted Deviation — FR-003 (recorded 2026-07-20, issue #752)

FR-003 requires the summary to "reuse the existing Inbox triage components (no
parallel UI)". The shipped implementation reuses the Inbox *data types*
(`InboxItemSummary`, `InboxClassifyResponse`) and, since #752, the shared
`Table` primitive — but **not** `InboxList`, `InboxDetail`, or
`InboxStatsSummary`. That deviation is accepted and scoped as follows.

### What now complies

The step's detection list renders through the shared `Table` primitive
(`apps/desktop/src/ui/Table.tsx`) — the same construct `InboxList` itself
renders through. The seven bespoke `pv-setup-scan__*` table classes that
re-implemented `.pv-table` were deleted; only `__table-wrap` remains, bounding
the scroll height of `Table`'s own `.pv-table__scroll` container.

### What deviates, and why

**`InboxList` cannot render the wizard's data.** It is typed on
`InboxListItem`, which extends `InboxItemSummary` with roughly fifteen
enrichment fields the wizard's scan never returns. `InboxList` reads exactly
those missing fields for its two headline columns: the primary detection column
reads `rootAbsolutePath`, and the Type column reads `frameType` /
`groupFrameType`. Feeding it wizard data would mean fabricating those fields and
still rendering both headline columns blank.

The mismatch is semantic, not cosmetic. `InboxList` models **single-type,
post-materialization** items — one authoritative `frameType` per row. The wizard
runs **before** materialization, over folders that are legitimately multi-type,
and presents a per-kind count breakdown ("16 light, 2 dark") sourced from a
separate `classifications` map rather than from item fields. `InboxList` has no
column that can express a breakdown. It also mandates `selectedId`/`onSelect`;
the wizard summary is read-only.

**`InboxStatsSummary`** is a flat stats strip, not a per-source container, so it
cannot host the wizard's grouped-by-source presentation. **`InboxDetail`** is
the triage editor for a selected item — the wizard performs no triage (FR-004,
FR-008), so it has no selected item to detail.

**The per-source accordion stays bespoke.** A shared collapsible primitive does
exist (`apps/desktop/src/ui/Section.tsx`, used by `InboxDetail` among others)
and was evaluated rather than assumed absent. It renders `{open && children}`
with no slot for always-visible body content. The wizard card must keep the
spec-048 US4 `RootDetectionConfig` control and the phase messages
(scanning/error/empty) visible *while collapsed*; adopting `Section` would
either hide that control behind the collapse or reorder it below the detail
table. Both are user-visible behaviour changes to a documented control, so they
are out of scope for a reuse refactor and would need their own iterate cycle.

Eighteen `pv-setup-scan__*` classes therefore remain, down from twenty-four.
Seventeen cover the accordion card, its phase messages, and its chips
(`__card`, `__header`, `__header--expandable`, `__chevron`, `__path`, `__kind`,
`__count`, `__msg` plus its three variants, `__detail`, `__chips`, `__chip`,
`__summary`, `__scroll`, `__empty-msg`); the eighteenth is `__table-wrap`.

### Status

Accepted. FR-003's intent — no cloned table markup or CSS — is met. The residual
deviation is the component-level reuse of `InboxList`/`InboxDetail`/
`InboxStatsSummary`, which is blocked by a genuine data-model difference rather
than by convenience. If the wizard ever runs post-materialization scans, this
decision should be revisited.
