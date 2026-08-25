> **MIGRATED:** current truth now lives at
> `docs/journeys/J09-targets-planning/journey.md`.
> This delta is frozen legacy history: it records behavior as understood on
> its own date and may not match current behavior.
> See `docs/product/journeys/README.md`.

# jval-docdrift — J09-targets-planning: 044/047 shipped — astronomy columns are real, not stubs (reconciles q16-t132/t133)

Task: jval-docdrift · Status: observed-live-2026-07-14

Journey-validation run 2026-07-14 (real Windows app, build 7e522c16) — observed doc-drift vs baseline.

## Behavior delta (this journey)

Baseline's entire "stubbed/pending" narrative (Stage 4 astronomy columns, Stage 5 favourites) is now **stale except Sessions**. Specs **044 Track B (astronomy-engine unification) and 047 Track A (Moon/filters) have SHIPPED**: Max altitude, Tonight's sparkline, Visible-tonight, Opposition, Lunar separation, recommended Filters, and Image time are now **REAL per-site astronomy-engine computations** against the configured observing site (via `settings_get`/`observingSites`) — not deterministic hash stubs. The **Sessions column is the only genuine remaining "—" stub** in this journey.

Baseline (Stage 5) says Favourites/"My Targets" is a browser-local (`localStorage`) preference only. The shipped app's favourites are **DB-backed** (`target_favourite` table, a real row with a timestamp), not localStorage-only.

Baseline's Known-gaps note says `aria-sort` on the Targets table "requires PR #415 (open)." **PR #415 is merged** — `aria-sort` works on the shipped table.

Baseline (Stage 1) says the Targets table "lists the seeded catalog (thousands of rows, virtualized for smooth scrolling)." The shipped app's main Targets table is actually **the user's added-target library** (a handful of rows) — the ~13k seed catalog is only searched via the Add-target typeahead and is never materialized as browsable rows.

**Planner math verified CORRECT**: the app's astronomy-engine computations agree with skyfield/DE421 and with Telescopius independently (e.g. M31 shows 0 imaging time on 2026-07-14 at 52.09°N because the Sun only reaches −16.4°, so there is no astronomical darkness that night). The one real defect found is a **graph-shading contradiction (#817)**: the altitude graph paints the usable-altitude fill under a high curve while omitting twilight shading when there's no dark window, making the target look imageable while imaging time is correctly computed as 0.

**Reconciliation note for reviewers**: this supersedes the "disclosed stub" framing in the existing Wave-0 deltas `2026-07-14-q16-t132.md` and `2026-07-14-q16-t133.md` for the astronomy columns specifically — those deltas' "placeholder astronomy" language now applies **only to the Sessions column**; the astronomy columns themselves are real per-site computations and should be verified as such, with #817 tracked separately as a rendering defect, not a stub-honesty defect.

## Stages hit

- Stage 1 "Targets lists the seeded catalog (thousands of rows, virtualized for smooth scrolling)" — the main table is actually the user's added-target library; the seed catalog is typeahead-only
- Stage 4 "The Targets table's astronomy-shaped columns … are not computed from real coordinates, date, or observer location yet. They are deterministic placeholders derived from a hash of the target's designation" — now real per-site astronomy-engine computations (044/047 shipped); Sessions is the only remaining stub
- Stage 5 "'Favourites'/'My Targets' is currently a browser-local (localStorage) preference only" — now DB-backed (`target_favourite` table)

## Reviewer verification

1. With an observing site configured (Settings → Observing Site), open Targets and assert Max altitude / Tonight / Visible / Opposition / Lunar separation / Filters / Image time vary meaningfully by target and site — not a stable hash-derived value across reloads.
2. Cross-check one target's computed values (e.g. M31 on 2026-07-14 at 52.09°N) against an independent ephemeris (skyfield/DE421) or Telescopius — expect agreement, including 0 imaging time when the Sun doesn't reach astronomical twilight.
3. Reproduce #817: confirm the altitude graph shades a usable-altitude fill under a high curve even when imaging time is 0 and no twilight window exists — verify as a rendering defect, not a stub-honesty defect.
4. Toggle a favourite, restart the app (or inspect the DB), and assert the `target_favourite` row persists with a timestamp — not merely `localStorage`.
5. Tab to the Targets table's active sortable column header and assert `aria-sort` is present.
6. Confirm the main Targets table row count matches the user's added targets, not the ~13k seed catalog; search "M31"/"Andromeda" in the Add-target typeahead to confirm the seed is reachable there instead.
7. Cross-check `2026-07-14-q16-t132.md` and `2026-07-14-q16-t133.md` in this journey's deltas folder — their "disclosed stub" framing for astronomy columns should now be read as superseded by this note; only the Sessions column retains stub framing.

## Rerun set (minimal)

- Layer-1: to be written in the owning task
- Layer-2: to be written in the owning task
- Manual-Windows: `journey-09-*` (targets) — re-walk with the live 2026-07-14 build; see also #817 for the graph-shading repro
- Coverage-matrix: #13
