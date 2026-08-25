# Feature Specification: Targets Planner — Track A (Moon-Aware, Filter-Aware Planning)

**Feature Branch**: `047-targets-planner-moon-filters`

**Created**: 2026-07-04

**Status**: Implemented (post-hoc record, verified 2026-08-24 — surfaces for all
four user stories located on `origin/main` a52c637f2; per-FR verification was not
performed, so treat individual FRs as unconfirmed rather than closed). Track A,
split out of the spec 044 placeholder; spec 044 is now Track B. US1 moon summary
`apps/desktop/src/features/targets/MoonSummary.tsx` over
`features/targets/astro/moon-state.ts` (phase / illumination); US2 real lunar
distance `features/targets/astro/lunar-separation.ts`; US3 per-band viability and
filter guidance `features/targets/GuidanceCell.tsx` and `FilterBadges.tsx`, with
the tunable per-band `(distance, width)` parameters in
`features/settings/PlannerSettings.tsx` over the `plannerMoonAvoidance` setting
(`crates/app/settings/src/descriptors.rs`); US4 opposition / best-season
`features/targets/astro/opposition.ts` (`nextOpposition:104`,
`oppositionRelative:161`) and `astro/best-moon-date.ts`. Each `astro/` module has
a colocated `.test.ts`. `iteration-2026-07-15-applied.md` in this directory
records a further applied iteration.

**Input**: User description: "Track A of the Targets Planner: moon-aware,
filter-aware target planning using existing target data and simple well-known
astronomy formulas. Track B (ephemeris/observer engine) is explicitly out of
scope and handled separately; Track A must not depend on it."

## Track Split (governing scope rule)

The spec 044 placeholder covered the whole planner-astronomy surface. That
scope is now split across two specs:

- **Track A (THIS spec — 047)** — everything computable from **date/time plus each
  target's catalogued sky coordinates (RA/Dec)** at *planning granularity*
  (whole-degree, whole-date precision). This covers Moon phase, Moon
  illumination, Moon sky position (geocentric), target↔Moon angular
  separation, moon-driven filter guidance, and opposition/best-season
  indication.
- **Track B (spec 044 — separate, research-gated effort, NOT this spec)** —
  everything requiring the **observer's location** (latitude/longitude/elevation/timezone
  precision): true altitude curves, max altitude tonight, imaging time above a
  threshold, visible-tonight determination, rise/set/transit times, moonrise
  and moonset, and observer-location capture itself.

**Boundary rule**: if a value cannot be computed without observer coordinates,
it belongs to Track B and stays a clearly-labeled placeholder in the planner
until Track B lands. Track A MUST NOT introduce any dependency on observer
location, and nothing in Track A blocks on Track B.

Planning-granularity tolerance is explicit and acceptable: the Moon's position
is evaluated geocentrically, which can differ from the observer's true
(topocentric) view by up to about 1 degree — irrelevant for deciding whether a
target is "near" or "far from" the Moon for filter selection.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Tonight's Moon at a glance (Priority: P1)

An astrophotographer opens the Targets Planner while deciding what to image
tonight. The planner shows the real Moon situation for tonight: phase name
(e.g., "Waxing Gibbous"), illumination percentage, and whether the Moon is
waxing or waning. The imager immediately knows whether tonight is a
"broadband night" (dark, near new Moon) or a "narrowband night" (bright Moon).

**Why this priority**: every other moon-aware feature derives from the nightly
Moon state; it is independently valuable even before any per-target column
exists, and it retires the first piece of fake data in the planner.

**Independent Test**: open the planner on a known date and compare the shown
phase name and illumination percentage against a published lunar almanac for
that date; repeat for dates around new Moon, full Moon, and both quarters.

**Acceptance Scenarios**:

1. **Given** tonight is a night near full Moon, **When** the user opens the
   Planner, **Then** the moon summary shows "Full Moon" (or the correct
   neighboring phase name) and an illumination percentage within tolerance of
   published almanac values for that date.
2. **Given** tonight is near new Moon, **When** the user opens the Planner,
   **Then** the summary shows a low illumination percentage and a
   new-Moon-adjacent phase name.
3. **Given** the app stays open across local midnight into the same night,
   **When** the user looks at the summary, **Then** the values still describe
   the same observing night (no mid-night flip to "tomorrow").

---

### User Story 2 - Real lunar distance per target (Priority: P1)

While comparing candidate targets, the imager sees, for each target row in the
planner, the real angular separation between that target and tonight's Moon,
and can sort the table by it — putting the most Moon-safe targets on top.

**Why this priority**: lunar separation is the per-target half of the
moon-avoidance decision and the direct replacement for the most misleading
mocked column. It requires only target RA/Dec (already available on every
planner row) plus the nightly Moon position from Story 1.

**Independent Test**: on a known date, compare the displayed separation for
several well-known targets (e.g., M31, M42) against a planetarium reference;
verify sorting orders rows by the displayed values.

**Acceptance Scenarios**:

1. **Given** a target with known catalogued coordinates, **When** the planner
   renders its row, **Then** the lunar distance column shows the real
   target↔Moon separation for tonight, in whole degrees, within the documented
   planning tolerance of a reference value.
2. **Given** the user clicks the lunar distance column header, **When** the
   sort applies, **Then** rows order by separation (ascending/descending
   toggle), with ties broken deterministically.
3. **Given** a target whose coordinates are not yet known to the library,
   **When** its row renders, **Then** the lunar distance shows an explicit
   "unknown" placeholder (not a fabricated number) and sorts after all rows
   with known values.

---

### User Story 3 - Filter guidance from Moon conditions (Priority: P2)

For each target, the planner shows per-band viability — for each filter band
in the fixed set L, R, G, B, Ha, SII, OIII, whether that band is workable
tonight given the Moon's brightness and the target's lunar distance — rendered
as compact per-band pills, plus a derived summary recommendation (broadband
OK / narrowband only / avoid tonight). The imager can filter the target list
to only the targets where their intended filter class works tonight. The user
can see why a recommendation was made and can tune the per-band avoidance
parameters that drive it.

**Why this priority**: this is the planner's headline decision-support value
("what can I shoot tonight with the Moon up?"), but it depends on Stories 1–2
landing first.

**Independent Test**: with a near-full-Moon date, verify targets close to the
Moon show only the moon-tolerant narrowband pills as viable and distant
targets show all bands viable; change per-band parameters in Settings and
verify pills and the derived recommendation update accordingly.

**Acceptance Scenarios**:

1. **Given** tonight's Moon is near full and a target's lunar distance is
   below a band's required minimum separation for tonight, **When** the row
   renders, **Then** that band's pill shows not-viable while bands whose
   requirement is met show viable, and the derived summary reads "narrowband
   only" when no broadband band is viable but at least one narrowband band is.
2. **Given** the Moon is near new OR the target is far enough from the Moon
   that every band's requirement is met, **When** the row renders, **Then**
   all pills show viable and the derived summary reads broadband OK
   (L/R/G/B and narrowband).
3. **Given** the user applies the planner's guidance filter set to
   "narrowband only", **When** the list refreshes, **Then** only targets with
   that derived recommendation remain visible.
4. **Given** the user inspects a row's guidance, **When** they hover or focus
   the guidance cell, **Then** an explanation states the inputs that produced
   it (tonight's Moon illumination/age, this target's lunar distance, and the
   per-band required separations from the active parameters).
5. **Given** the user changes any band's avoidance parameters in Settings,
   **When** they return to the planner, **Then** pills and recommendations
   reflect the new parameters, and a "reset to defaults" action restores the
   shipped values.
6. **Given** a target with unknown coordinates, **When** its row renders,
   **Then** the guidance shows an explicit "unknown" state rather than a
   recommendation.

---

### User Story 4 - Opposition / best-season indication (Priority: P3)

The imager sees, per target, when that target is next "in season" — the date
it culminates near local midnight (its opposition-like best-visibility
moment) — and can sort by it to find targets peaking soon. This replaces the
current empty "—" opposition column with real, date-level values.

**Why this priority**: valuable for seasonal planning but less
decision-critical night-to-night than the Moon columns; it is independent of
the Moon work and only needs target RA plus the Sun's seasonal position at
date-level precision.

**Independent Test**: verify well-known seasonal anchors (e.g., Orion-region
targets culminate near midnight in December, Sagittarius-region targets in
June/July) within the documented date tolerance, and that sorting orders rows
by soonest-next-opposition.

**Acceptance Scenarios**:

1. **Given** a target with known coordinates, **When** its row renders,
   **Then** the opposition column shows the next date (at date-level
   precision) the target culminates near local midnight, replacing the "—"
   stub.
2. **Given** the user sorts by opposition, **When** the sort applies, **Then**
   rows order by how soon that date arrives (targets at/near opposition now
   first), deterministically.
3. **Given** a target with unknown coordinates, **When** its row renders,
   **Then** the opposition column shows the explicit "unknown" placeholder and
   sorts last.

---

### Edge Cases

- Target exactly at a band's required minimum separation for tonight:
  viability MUST be deterministic and documented (separation equal to the
  requirement counts as viable: `separation ≥ min_separation` → viable).
- Targets missing coordinates (never resolved, or user-created without
  lookup): all Track A columns show an explicit unknown state, never invented
  numbers; unknowns group after known values under any sort.
- Night spanning local midnight: all Track A values for "tonight" refer to one
  observing night, not the calendar date, so nothing flips at 00:00 while the
  user is planning.
- Date/clock changes (system timezone change, DST transition, user returns
  the next day with the app still open): the planner presents the correct
  current observing night after refresh/reopen; a stale night is never mixed
  with a fresh one within one rendered view.
- Extreme dates far in the past/future: values stay plausible (no crashes or
  nonsense such as illumination outside 0–100% or separation outside 0–180°).
- Per-band parameters set to extremes (e.g., required separation at full Moon
  = 180°, or halving width at its minimum): guidance follows the configured
  rule literally; the settings surface constrains inputs to valid ranges so
  no invalid state is storable.
- No observing site configured yet (fresh install pre-wizard, or wizard
  skipped): the planner presents an explicit "set up your observing site"
  prompt state instead of rendering astronomy values; no
  location-independent fallback rendering is offered.

## Requirements *(mandatory)*

### Functional Requirements

**Nightly Moon state**

- **FR-001**: The system MUST compute, for the current observing night, the
  Moon's phase (including waxing/waning direction) and illumination fraction
  from date/time alone, using well-established published low-precision lunar
  formulas, with no dependency on observer location.
- **FR-002**: The planner MUST display a moon summary for tonight: phase name
  (standard eight-phase vocabulary), illumination percentage, and
  waxing/waning direction.
- **FR-003**: Illumination MUST be within ±3 percentage points, and the phase
  name correct or adjacent-boundary-correct, versus published almanac values
  for any date within at least the 2000–2050 range.
- **FR-004**: The system MUST compute the Moon's sky position for the night at
  planning granularity (geocentric; documented tolerance approximately 1°
  versus a topocentric view) for use in per-target separation.
- **FR-005**: All Track A values shown together at one time MUST describe the
  same single observing night, defined as the night containing the upcoming
  (or in-progress) local midnight; values MUST NOT flip mid-night at 00:00.

**Per-target lunar distance**

- **FR-006**: For every planner row with known target coordinates, the system
  MUST show the real target↔Moon angular separation for tonight, in whole
  degrees (0–180°), replacing the previous placeholder values.
- **FR-007**: Lunar distance MUST be sortable ascending/descending with
  deterministic tie-breaking; rows with unknown coordinates MUST show an
  explicit unknown state and group after all known values.
- **FR-008**: Displayed separations MUST be within ±2° of reference
  planetarium values for the same date (planning tolerance, inclusive of the
  geocentric simplification).

**Filter guidance**

- **FR-009**: The system MUST derive per-band viability for tonight, for the
  fixed band set L, R, G, B, Ha, SII, OIII, using the Moon-avoidance
  Lorentzian rule: a band is viable for a target when the target's lunar
  distance is at least `distance_b / (1 + (age / width_b)²)`, where `age` is
  days from full Moon (derived from tonight's Moon state), and `distance_b`
  (required separation at full Moon, degrees) and `width_b` (days from full
  at which the requirement halves) are per-band parameters. Separation equal
  to the requirement counts as viable. This replaces the previous placeholder
  rule with real inputs. The band set is fixed in v1 (not derived from the
  user's filter inventory) and there is no narrowband/broadband mode
  selector; target type does not modulate the rule.
- **FR-009a**: The planner MUST render per-band viability as compact pills in
  the row's filter-guidance column, plus a derived summary recommendation:
  broadband OK (all bands workable), narrowband only (no broadband band
  viable, at least one narrowband band viable), avoid tonight (no band
  viable), or unknown.
- **FR-010**: The per-band parameters (`distance_b`, `width_b` for each of
  the seven bands) MUST be user-configurable via a compact per-band table in
  Settings → Target Planner, constrained to valid ranges, MUST persist across
  restarts, and MUST offer a reset-to-defaults action. Shipped defaults:
  L/R/G/B 120°/14d; Ha/SII 60°/7d; OIII 110°/10d.
- **FR-011**: The user MUST be able to filter the planner's target list by
  derived recommendation category, and the existing group-by-recommendation
  behavior MUST operate on the real values.
- **FR-012**: Each recommendation MUST be explainable in place: the user can
  see the inputs (tonight's illumination/Moon age, this target's separation)
  and the per-band required separations produced by the active parameters.
- **FR-013**: Targets with unknown coordinates MUST show an unknown guidance
  state, excluded from recommendation-based filters except via an explicit
  "unknown" choice.

**Opposition / best season**

- **FR-014**: For every planner row with known coordinates, the system MUST
  show the next date (date-level precision, tolerance ±7 days) the target
  culminates near local midnight, derived from the target's catalogued
  position and the Sun's seasonal position — no observer coordinates —
  replacing the "—" stub. The column MUST be sortable by soonest-next.
  *(Iteration 2026-07-15)*: the opposition column is retained in the
  consolidated planner table and MUST be sized to its real content
  ("14 Apr · in 9 months") per spec 044's content-driven sizing requirement
  (closes #792); soonest-next sort semantics (SC-003) are unchanged.
  *(Iteration 2026-07-17)*: the LIST opposition column remains this pure
  geometric transit-at-midnight date. Spec 044's detail-panel Moon-aware
  best date (044 FR-009 amendment) consumes this opposition search and the
  shared Lorentzian rule (FR-009) as-is — it never redefines the rule, the
  parameters, or this column's value/sort.

**Boundary and placeholder integrity**

- **FR-015** *(superseded, iteration 2026-07-15)*: ~~The planner columns that
  require observer location — max altitude, imaging time tonight, visible
  tonight, and the per-row altitude sparkline — MUST remain in place but stay
  clearly presented as estimates/placeholders, unchanged in behavior, until
  Track B delivers real values.~~ Track B (spec 044) shipped real values, and
  the 044 observability iterate (D4, resolved Q2) removes the per-row
  altitude sparkline column and folds visible-tonight into the imaging-time
  glyph — the "remain in place unchanged" obligation no longer binds. The
  enduring half stands: Track A MUST NOT request or use observer location.
- **FR-016** *(fulfilled, iteration 2026-07-15)*: The existing usable-altitude
  threshold setting was retained and has carried over to Track B as planned
  (spec 044 FR-004 owns it now); historical.
- **FR-017**: After Track A ships, no moon-related or opposition value in the
  planner may be derived from placeholder/deterministic-hash sources; every
  displayed Track A number is a real planning-granularity computation or an
  explicit unknown state.
- **FR-018**: All new visible text MUST follow the product's existing
  localization approach, and new columns/controls MUST remain operable
  keyboard-first with correct sort-state announcement, consistent with the
  product's accessibility baseline (WCAG AA).
- **FR-019**: The planner MUST NOT render Track A astronomy values until an
  observing site exists (prompt-for-site-first): when no default observer
  site is configured, the planner shows an explicit prompt to complete site
  setup instead of the astronomy columns/summary. Track A only CONSUMES the
  site-existence signal (the default site is created by the setup-wizard work
  coordinated under Track B/spec 048); Track A does not build the wizard step
  and does not use the site's coordinates in any computation.
- **FR-020** *(added, iteration 2026-07-15 — 044 D7)*: Rule consumers MAY
  aggregate the per-band required separations across an OSC camera's
  passband by taking the strictest (maximum) `minSeparationDeg` over the
  passband's bands for a given Moon age. The aggregation lives on the
  consumer (Track B / spec 044) side; this spec's rule, per-band
  `(distance, width)` parameters, shipped defaults, and viability pills are
  unchanged, and no scalar aggregate parameter is introduced (the
  `min_lunar_separation_deg` scalar remains rejected — see plan.md:255-260
  and contracts/settings.plannerMoonAvoidance.md).

### Key Entities

- **Observing Night**: the single night all planner values describe; carries
  the Moon's phase name, waxing/waning direction, illumination fraction, and
  planning-granularity sky position for that night. One per night; shared by
  the summary and every row.
- **Target Planning Row**: a library target as seen in the planner; key
  planning attributes: designation/label, catalogued coordinates (may be
  unknown), lunar distance tonight, filter recommendation tonight, next
  opposition date, plus the existing Track-B placeholder fields.
- **Filter Guidance Policy**: the per-band Moon-avoidance parameter table —
  for each band in the fixed set {L, R, G, B, Ha, SII, OIII} a
  (`distance`, `width`) pair with shipped defaults and valid ranges — plus
  the Lorentzian rule that turns (Moon age, lunar distance) into per-band
  viability and a derived summary recommendation (broadband-OK,
  narrowband-only, avoid-tonight, unknown). Shared as ONE frontend rule
  module with Track B (spec 044), which integrates the same rule over its
  observer-location geometry; spec 047 owns the rule and its parameters.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For any date between 2000 and 2050, the planner's Moon
  illumination is within ±3 percentage points of published almanac values,
  and the phase name matches (or is boundary-adjacent within one day of a
  phase transition).
- **SC-002**: For a benchmark set of at least 10 well-known targets across
  the sky on at least 5 spread dates, displayed lunar distances are within
  ±2° of planetarium reference values in 100% of cases.
- **SC-003**: For a benchmark set of seasonal anchor targets, the opposition
  date is within ±7 days of reference best-visibility dates in 100% of cases.
- **SC-004**: Zero planner values in the moon/filter/opposition columns
  originate from placeholder generators after release; audit of the planner
  on any date shows real values or explicit unknown states only.
- **SC-005**: A user can go from opening the planner to a Moon-safe shortlist
  (sorted by lunar distance, filtered to their filter class) in under 30
  seconds using only planner controls.
- **SC-006**: 100% of guidance recommendations expose an in-place explanation
  of their inputs and thresholds.
- **SC-007**: With a library of 5,000 planner targets, enabling the new
  columns does not perceptibly degrade planner responsiveness (interaction
  remains under typical desktop-app responsiveness expectations, e.g.,
  sorting completes without visible stall).
- **SC-008**: Changing any band's guidance parameter updates every visible
  pill and recommendation without restart, and reset-to-defaults restores
  shipped behavior in one action.

## Assumptions

- **Tonight-only planning**: Track A plans for the current observing night
  only. Choosing an arbitrary future night (date picker) is out of scope
  unless promoted later (flagged as an open question).
- **Observing-night definition**: "tonight" is the night containing the
  upcoming or in-progress local midnight, using the system clock/timezone;
  the nightly Moon state is evaluated once per night at local midnight,
  which is sufficient at planning granularity (the Moon moves ~0.5°/hour;
  well inside the ±2° separation tolerance across a session).
- **Guidance model is per-band Lorentzian and Moon-only**: per-band viability
  from the Moon-avoidance Lorentzian rule (the model used by ACP Scheduler
  and NINA's Target Scheduler), driven solely by Moon age/illumination and
  separation. Target type/brightness (e.g., "galaxies don't benefit from
  narrowband") does NOT influence Track A guidance — galaxies are handled
  naturally by reading the L/R/G/B pills; no target-type auto-modulation and
  no narrowband/broadband selector in v1.
- **Fixed filter vocabulary**: the band set is the fixed broadband set
  L/R/G/B and narrowband set Ha/SII/OIII; it is not derived from the user's
  actual filter inventory in session metadata.
- **Default parameters**: L/R/G/B `distance` 120° / `width` 14d; Ha/SII
  60°/7d; OIII 110°/10d (OIII is empirically the most Moon-sensitive
  narrowband band) — per the spec-044 astronomy-libraries research;
  user-adjustable per FR-010.
- **Site gate, not site input**: the planner requires an observing site to
  exist before rendering astronomy (product decision shared with Track B),
  but no Track A computation reads the site's coordinates; "tonight" uses the
  system clock/timezone.
- **Coordinates are available**: target coordinates (RA/Dec) are already
  supplied per planner row by the existing target list (the former
  enrichment blocker is resolved); Track A consumes them and never looks up
  coordinates itself. Rows can still legitimately lack coordinates.
- **Opposition presentation**: shown at date-level precision (e.g., month +
  day); the exact display form (absolute date vs relative "in N months") is
  a presentation decision deferred to design, but sorting is by
  soonest-next in all cases.
- **Moonrise/moonset excluded**: rise/set times require observer location →
  Track B, so the Track A moon summary deliberately omits them.
- **No processing**: the planner recommends and organizes only; it never
  processes, calibrates, or edits images (PixInsight boundary preserved).
- **Existing planner behaviors retained**: catalogue filtering, grouping,
  designation sort, and the usable-altitude setting continue unchanged;
  Track A only replaces the values feeding the moon/filter/opposition
  columns and adds the moon summary.

## Out of Scope (Track B and beyond)

- Observer location capture (settings, geolocation, per-root) and anything
  derived from it: altitude curves, max altitude, imaging time, visibility,
  rise/set/transit, moonrise/moonset, twilight times (dusk/dawn windows).
- Arbitrary-night planning (date picker), multi-night calendars, session
  scheduling.
- Telescopius-parity guidance modeling that weighs target type, surface
  brightness, or the user's filter inventory.
- Any image processing, stacking, or calibration (constitutional boundary).
