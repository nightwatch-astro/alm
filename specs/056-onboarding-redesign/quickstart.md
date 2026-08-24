# Quickstart: Onboarding Redesign (Spec 056)

Orient an implementing agent in ~2 minutes. Details: [plan.md](plan.md),
[research.md](research.md), [data-model.md](data-model.md),
[contracts/onboarding-commands.md](contracts/onboarding-commands.md).

## What this is

Three-layer onboarding replacing the spec 010 coach:

- **L1 Orientation walk** — modal joyride tour, once, right after first-run
  setup: 6 stops = the 5 workflow pages + a final stop on the sidebar Getting
  started section (L1→L2 bridge); replay in Settings → Advanced.
- **L2 Getting Started checklists** — one sidebar accordion (Inbox, Sessions,
  Calibration, Targets, Projects), backend-authoritative auto-ticks from real
  bus events, manual check/dismiss for the rest; completed groups collapse to
  a done header, fully-complete section auto-hides; explicit remove + Settings
  restore (restore re-derives automatic items only — manual states preserved).
- **L3 Find spotlight** — non-modal single-step joyride spotlight on the real
  control, five dismissal paths, never timeboxed.

## Non-negotiables

1. Ticks are written by a Rust bus subscriber, never the frontend
   (`source=="restore"` filtered server-side). Frontend reads
   `onboarding_state_get`, refreshes on `onboarding:state-changed`.
2. Verified auto-tick topics only ([research R4](research.md)):
   `inventory.confirmed`, `project.created`, `tool.launch`
   (outcome==spawned), `plan.applying.completed`, `target.resolved`. No new
   events in v1 — master-registered and site-saved items are MANUAL.
3. Joyride adapter rules (spike-verified, [research R2](research.md)): custom
   `tooltipComponent` that does NOT spread `tooltipProps`; own aria-live
   announcer; focus trap ON for the walk, `disableFocusTrap` for spotlights;
   ALWAYS gate `run={steps.length > 0}`.
4. Delete the old machinery per [research R7](research.md) — do not adapt it.
   Keep `data-guide-anchor`; single `inbox.confirm-row` anchor lives on
   InboxPage bulk-confirm.
5. No demo data, ever (FR-009). Every string via Paraglide
   (`apps/desktop/messages/en.json`). ONE parameterised checklist component +
   one CSS class family.
6. Migration `0080_onboarding.sql` creates onboarding state. Migration
   `0081_drop_guided_flow_state.sql` drops `guided_flow_state` and leaves
   `0030_guided_flow.sql` untouched.

## Where things go

See plan.md "Project Structure". New code: `crates/app/core/src/onboarding.rs`,
`crates/contracts/core/src/onboarding.rs`,
`crates/persistence/lifecycle/src/repositories/onboarding.rs`,
`apps/desktop/src-tauri/src/commands/onboarding.rs`,
`apps/desktop/src/features/onboarding/`.

## Verify

- `just lint` / `just test` / `just typecheck`.
- Layer-1: event publish → subscriber tick → restore inert (VC-003).
- Playwright mock suite: walk incl. skip, accordion, dismissal matrix,
  persistence, a11y (VC-002; auto-tick NOT testable in mock mode — documented).
- Layer-2 journey: orientation → real confirm → live auto-tick (VC-004).
- Journey J18 is the behavioral contract (VC-001); coverage-matrix row per
  VC-005. E2E suites use the new suppression flag (`disableOnboarding`,
  [research R8](research.md)).
