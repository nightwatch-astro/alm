# Feature Specification: Targets Planner — Ephemeris & Observer-Location Engine (Track B)

**Feature Branch**: `044-targets-planner-track-b`

**Created**: 2026-07-04

**Status**: Implemented (post-hoc record, verified 2026-08-24 — surfaces for all
six user stories located on `origin/main` a52c637f2; per-FR verification was not
performed, so treat individual FRs as unconfirmed rather than closed). US1 engine
`apps/desktop/src/features/targets/planner-astronomy.ts:256`
`computeNightObservability` with `usableAltDeg` now a prop defaulting to
`USABLE_ALT_DEG` (`TargetsTable.tsx:203`, `TargetDetail.tsx:88`); US2
`PlannerDatePicker.tsx`; US3 `features/settings/ObservingSites.tsx` over the
`observerSites` setting (`crates/app/settings/src/descriptors.rs`); US4 per-site
`twilight` / `minHorizonAltDeg` in that same descriptor; US5
`features/targets/astro/lunar-separation.ts` plus the registered
`target_moon_opposition_batch` command (`bootstrap/specta.rs:138`, `:265`); US6
first-run default site `features/setup/steps/StepSite.tsx`. Supporting modules
`astro/observing-night.ts`, `astro/opposition.ts`, `astro/best-moon-date.ts` and
`astro/row-planning.ts`, each with a colocated `.test.ts`.

**Implementation location** (resolved after the fact; this spec deliberately left
it to plan/research): the ephemeris engine is **frontend TypeScript** under
`apps/desktop/src/features/targets/`, not a Rust crate. There is no observer-site
Tauri command; sites are a settings key described in
`crates/app/settings/src/descriptors.rs`.

**Input**: User description: "Give the Targets planner real, per-site, per-date observability. For each deep-sky target (and the Moon), compute where it is in the sky over a chosen night from a chosen observing site: altitude over time, when it transits, when it rises/sets, its best imaging date, and its geometry against the Moon — so the planner can show real max-altitude, altitude sparkline, visible-tonight, imaging-time and per-filter moon-free time instead of placeholders. Let the user keep several named observing sites, pick which is active, choose how dark 'night' must be and how low the horizon reaches, set the lowest usable altitude, and plan any date — with a default site created in the first-run wizard so the planner works out of the box."

## Context & Motivation

The Targets planner table and the target detail view already expose columns and an altitude graph for observability — maximum altitude, an altitude sparkline over the night, a "visible tonight" flag, imaging time above a usable altitude, transit / best-date, Moon distance, and a filter recommendation — but at the time this spec was written those values were driven by placeholder logic: the usable-altitude cutoff was a hardcoded constant (`USABLE_ALT_DEG`), there was no real observing site behind the numbers, "tonight" was the only planning horizon, and the Moon figure was not a real angular separation. The planner looked like a planning tool without yet being one. (All four conditions have since been removed — see the Status header.)

This feature (Track B) provides the real **ephemeris and observer-location engine** behind those surfaces: given an observing site and a date, it computes where each target is over the night — altitude samples, transit, rise, set, best imaging date — and the Moon's real geometry over that night (its altitude and its angular separation from each target). From that geometry it derives the total imaging time above the usable altitude and, integrating the filter track's per-band Moon-avoidance rule over the night, a **per-filter moon-free imaging time**. It also introduces the user-facing model the engine needs: multiple named observing sites with a default and an active choice, a per-site definition of how dark "night" must be and how low the horizon reaches, a configurable usable-altitude threshold, and a **default site created in the first-run wizard** so the planner is populated from the start.

### Track A (spec 047) / Track B (this spec) boundary

The astronomy work for the planner is split into two independently specified tracks that share one common astronomy-math capability and one shared Moon-avoidance rule:

- **Track B (this spec)** owns everything about **time × observer-location × position**: per-target altitude over the night, transit / rise / set times, best imaging date, the night-darkness window, the observing-site model (incl. the first-run wizard default site), the usable-altitude threshold, the **Moon's geometry over the night** (its altitude and its angular separation from each target, three summary separation figures, and Moon-up windows), the band-free **total imaging time**, and — by integrating Track A's Moon-avoidance rule over that geometry — the **per-filter moon-free imaging time**.
- **Track A (spec 047)** owns the **Moon-avoidance product rule** — the per-band Lorentzian `min_sep(moonAge) = distance / (1 + (moonAge/width)²)` with user-tunable per-band `(distance, width)` parameters (Ha/SII/OIII/L/R/G/B) — plus Moon phase / illumination, the per-band viability pills, and the filter recommendation. The rule and its parameters live in one shared frontend module and shared settings; Track B **consumes** that rule to integrate per-band moon-free time, but does not define the filter tolerances or the recommendation.

The rule of thumb: anything that needs the Moon *above the user's horizon* or a target's *position for a specific site and time* — including integrating a tolerance over the night — is Track B; the *definition* of the per-band tolerance and the filter advice is Track A. Both tracks draw on one shared astronomy-math capability whose internals (which algorithms or library) are a **plan / research concern**, intentionally unspecified here so this document stays user- and workflow-focused per the constitution.

### Instant threshold updates

Observability has a part that depends only on *where a target is and when* (altitude samples, transit, rise, set, Moon geometry) and a part that depends on *thresholds*. Changing the **usable-altitude threshold** MUST update the total-imaging-time and visible-tonight columns immediately, and changing Track A's **per-band Moon-avoidance parameters** MUST update the per-filter moon-free times immediately — both derived from the already-computed positions without recomputing where anything is in the sky. (How and where the derivation runs is a plan concern.)

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Real tonight observability per target (Priority: P1)

An astrophotographer opens the Targets planner for their default observing site and sees, for each deep-sky target, real numbers for tonight: the maximum altitude the target reaches, an altitude sparkline across the night, whether it is worth imaging tonight, the imaging time it spends above the usable altitude, and when it transits. They drag the usable-altitude slider and the imaging-time and visible-tonight values update instantly.

**Why this priority**: This is the core value — turning the planner from placeholder columns into a real per-night observability view. It is the smallest slice that makes the planner trustworthy and is a viable MVP on its own (using the default site and tonight's date).

**Independent Test**: With a default observing site configured, load the planner and confirm each target's max altitude, altitude sparkline, transit time, visible-tonight flag, and imaging-time-above-threshold match an independent ephemeris for that site and tonight; then move the usable-altitude slider and confirm the imaging-time and visible-tonight columns change without re-fetching positions.

**Acceptance Scenarios**:

1. **Given** a default observing site and a target with known coordinates, **When** the planner loads for tonight, **Then** the target's maximum altitude, transit time, and altitude sparkline match an independent ephemeris to planning-grade accuracy.
2. **Given** the planner is showing tonight's observability, **When** the user changes the usable-altitude threshold, **Then** the total-imaging-time and visible-tonight values recompute immediately from the existing positions with no position recomputation.
3. **Given** a target that never rises above the usable altitude tonight from the active site, **When** the planner loads, **Then** the target is marked not visible tonight with zero imaging time, without error.

---

### User Story 2 - Plan an arbitrary future night (Priority: P2)

The user is deciding which night next month to image a target. They pick a future date in the planner and every observability column and the altitude graph update to that night, for the active site, so they can compare candidate nights.

**Why this priority**: Arbitrary-date planning is what makes the tool a *planner* rather than a *tonight dashboard*, but it builds directly on US1 and is not required for the first useful slice.

**Independent Test**: Change the planner's date to a specific future date and confirm the max altitude, transit, rise/set, and altitude sparkline for a target match an independent ephemeris for that date and the active site (not tonight's values).

**Acceptance Scenarios**:

1. **Given** the planner is set to the active site, **When** the user selects a future date, **Then** all observability values and the altitude graph reflect that date's night for that site.
2. **Given** a target with a computed best imaging date, **When** the user navigates to that date, **Then** the target transits near local midnight (anti-solar) as indicated.
3. **Given** the app has just been (re)launched, **When** the planner opens, **Then** the planning date defaults to "tonight" for the active site (the date is not carried over from a prior session).

---

### User Story 3 - Manage observing sites (Priority: P2)

The user keeps more than one observing location (a home site and a dark-sky site). They add, edit, and delete named sites — entering latitude, longitude, elevation, and picking a timezone from a list — mark one as the default, and switch which site the planner is currently computing for. Observability across the whole planner follows the active site, and the active site persists across relaunches.

**Why this priority**: Multiple sites are essential for real users but a single default site is enough to deliver US1; full site management is the next increment.

**Independent Test**: Create two sites with different coordinates, set one active, verify the planner's numbers match that site; switch to the other and verify the numbers change; relaunch and confirm the active site is remembered; delete a non-default site and confirm the active/default selection remains valid.

**Acceptance Scenarios**:

1. **Given** at least one site exists, **When** the user adds another site with a name, latitude, longitude, elevation, and a timezone chosen from a list, **Then** it is persisted and can be marked default and made active — with no online lookup required.
2. **Given** two sites exist, **When** the user switches the active site, **Then** every observability value in the planner recomputes for the newly active site.
3. **Given** a site is marked default, **When** the user deletes a different (non-default) site, **Then** the default and active selections remain valid and the deleted site no longer appears.
4. **Given** an active site was chosen, **When** the app is relaunched, **Then** the same site is active.

---

### User Story 4 - Choose how dark "night" is and how low the horizon reaches (Priority: P3)

The user sets, per site, whether the observable night is bounded by astronomical twilight (a fully dark sky) or nautical twilight (a looser bound), and a minimum usable horizon altitude for local obstructions (trees, buildings). The night window used for imaging-time and the altitude graph shading follows the twilight choice, and rise/visibility respect the minimum horizon.

**Why this priority**: A sensible default (astronomical twilight, flat 0° horizon) works for most users; per-site overrides are refinements that matter mainly for high-latitude, bright-target, or obstructed-site cases.

**Independent Test**: For a site and date, switch the twilight definition from astronomical to nautical and confirm the night window (and therefore imaging-time and the graph's night shading) widens; raise the site's minimum horizon altitude and confirm a low-transiting target's usable/visible time shrinks accordingly.

**Acceptance Scenarios**:

1. **Given** a site set to astronomical twilight, **When** the planner computes the night window, **Then** the window runs between the sun reaching −18° in the evening and rising back to −18° in the morning.
2. **Given** the user switches that site to nautical twilight, **When** the planner recomputes, **Then** the night window uses the sun at −12° and the imaging-time figures update to the wider window.
3. **Given** a site with a raised minimum horizon altitude, **When** a target only ever reaches just above the true horizon, **Then** its usable/visible time reflects the raised horizon rather than a flat 0°.

---

### User Story 5 - Real Moon geometry and per-filter moon-free time (Priority: P2)

For each target, the user sees the real Moon geometry for the planned night from the active site — the target-to-Moon separation (at transit, closest approach during dark, and at the dark-window midpoint) and when the Moon is above the horizon — and a **per-filter moon-free imaging time** (e.g. "Ha 4.2h · OIII 2.1h · LRGB 0h"): the hours above the usable altitude in the dark window during which the Moon is not close enough to hurt that band, according to the filter track's per-band Moon-avoidance rule. The altitude sparkline shades usable-uptime and, for a chosen band, the Moon-interference intervals.

**Why this priority**: Moon proximity is a primary go/no-go factor, and it differs sharply by filter — a single number misleads. The per-filter moon-free time is what turns "how close is the Moon" into "how much of tonight is usable, for the filter I'll actually shoot."

**Independent Test**: For a target and the Moon on a given night and site, confirm the three separation figures and the Moon-up windows match an independent ephemeris; confirm each band's moon-free time equals the summed dark-window intervals where the target is above the usable altitude and NOT (Moon above horizon AND separation below that band's Moon-avoidance minimum for the night's Moon age), using Track A's rule and parameters.

**Acceptance Scenarios**:

1. **Given** a target and a date/site, **When** the planner computes Moon geometry, **Then** the target-to-Moon separation at transit, its minimum over the dark window, and its value at the dark-window midpoint each match an independent ephemeris to planning-grade accuracy.
2. **Given** the Moon is above the horizon and within a band's Moon-avoidance minimum during part of the dark window, **When** that band's moon-free time is computed, **Then** it excludes that interval; a more Moon-tolerant band (e.g. Ha) excludes less than a sensitive one (e.g. OIII or LRGB).
3. **Given** the Moon is below the horizon for the whole night, **When** moon-free time is computed, **Then** every band's moon-free time equals the band-free total imaging time and separation reads "Moon not up" where relevant.
4. **Given** the user changes a band's Moon-avoidance `(distance, width)` parameters, **When** the planner recomputes, **Then** that band's moon-free time updates immediately without recomputing positions.

---

### User Story 6 - Works out of the box: default site from the setup wizard (Priority: P1)

On first run the setup wizard captures a default observing site (name, latitude, longitude, timezone; elevation optional, prefilled from imported session metadata when available), so the planner has real astronomy immediately. If somehow no site exists, the planner shows a clear "add an observing site" prompt and no fabricated astronomy rather than blank or wrong numbers.

**Why this priority**: Without a site there is no observer, so no real observability at all — the entire feature is gated on a site existing. Capturing it in the wizard is what makes US1 true "out of the box," so this is co-P1 with US1.

**Independent Test**: Run the first-run wizard, complete the observing-site step, and confirm a default+active site is persisted and the planner immediately shows real numbers; separately, with no site configured, confirm the planner shows the add-a-site prompt and no astronomy.

**Acceptance Scenarios**:

1. **Given** a first run, **When** the user completes the wizard's observing-site step (name, lat, lon, timezone; elevation optional), **Then** a site is persisted as both default and active and the planner shows real observability with no further setup.
2. **Given** the wizard step and available imported session observer metadata, **When** the step opens, **Then** it MAY prefill latitude/longitude/timezone for the user to confirm and complete.
3. **Given** no observing site exists, **When** the planner is opened, **Then** it shows a clear prompt to add a site and renders no astronomy values rather than placeholders.

---

### Edge Cases

- **No astronomical dark (high-latitude summer)**: When the sun never reaches the chosen twilight depression, the night window is empty. The engine MUST report that there is no dark window for that site/date rather than fabricating one; total imaging time and every band's moon-free time MUST be zero.
- **Circumpolar targets**: A target that never sets has no rise/set time; the engine MUST represent transit and altitude samples normally and mark rise/set as not applicable rather than erroring.
- **Never-rises / never-usable targets**: A target that never clears the horizon (or the usable altitude / minimum horizon) MUST be marked not visible with zero imaging time, with no rise/set/transit failure.
- **No active/default site**: With no sites configured, the planner MUST degrade to a clear "add an observing site" state (US6) rather than showing wrong or blank numbers.
- **DST / timezone boundaries on arbitrary dates**: When a planned night crosses a daylight-saving transition (or the site's timezone rules differ from the app host's), rise/set/transit and the night window MUST be reported in the site's local time correctly across the transition.
- **Moon up but far from target**: The Moon-up windows and separation series MUST be reported as raw geometry; whether that proximity costs a given band is the per-band rule's integration, and a band with a small `distance` may be unaffected while a sensitive band is not.
- **Target coordinates missing**: A target without resolved coordinates cannot be placed; the planner MUST show it as un-plannable (needs coordinates) rather than computing against a default.
- **Simultaneous zero-blockers (iteration 2026-07-15)**: When several blockers are true at once (e.g. high-latitude summer: no dark window AND the target never clears the threshold), the stated reason follows the FR-029 precedence (darkness > altitude > moon) — one reason is reported, deterministically.
- **OSC narrowband on a moonlit night (iteration 2026-07-15)**: The single-pass headline may read 0/low while a tolerant line (e.g. Ha) is individually viable; the detail panel's per-line breakdown (FR-037) MUST disclose that rather than letting the headline read as "night useless".

## Requirements *(mandatory)*

### Functional Requirements

**Tonight observability (US1)**

- **FR-001**: For a given observing site and date, the system MUST compute, for each deep-sky target with known coordinates, altitude samples across the night sufficient to render an altitude curve and to derive maximum altitude.
- **FR-002**: The system MUST compute each target's transit time (moment of maximum altitude) for the site and date.
- **FR-003**: The system MUST compute each target's rise and set times where they exist (respecting the site's minimum horizon altitude and standard atmospheric refraction), and MUST represent their absence for circumpolar and never-rising targets without error.
- **FR-004**: The system MUST expose a configurable **usable-altitude threshold** (lowest altitude at which imaging is worthwhile), a **global** user setting defaulting to 30°, replacing the current hardcoded constant.
- **FR-005** *(amended, iteration 2026-07-15 — D1)*: The system MUST derive, for each target, the band-free **total imaging time** above the usable-altitude threshold during the night's dark window, and a **visible-tonight** indication, from the computed altitude samples and the night window. The planner MUST distinguish, per target/site/date, three physically distinct quantities: (a) the **astronomical darkness window** (a function of site+date only — the night's dark window per FR-015/FR-017); (b) the **target uptime window** (a function of target+site+date — above horizon plus the usable-altitude threshold); and (c) **imaging time** = (a) ∩ (b) ∩ per-band moon-viability. The target detail panel MUST expose (a) and (b) as distinguishable facts, not only their intersection. No new math — these are already-computed intermediates.
- **FR-006**: The usable-altitude-dependent values (total imaging time, visible-tonight) MUST derive from already-computed positions so that changing the usable-altitude threshold updates them immediately, without recomputing target or Moon positions.
- **FR-007** *(amended, iteration 2026-07-15 — D4/D6, resolved Q2/Q3)*: The altitude samples and derived observability MUST be presented in the planner table and the target detail altitude graph, replacing placeholder values. The planner table carries **no per-row altitude sparkline column** (hard removal — the detail panel's full altitude graph is the canonical altitude view) and **no dedicated visible-tonight column** (folded into the imaging-time glyph, FR-030/FR-031); the surviving columns are Designation, Type, Max alt, Opposition, Lunar dist, Filters, and Imaging time. The detail altitude graph MUST overlay the twilight/darkness bands, the usable-altitude threshold line, and the Moon-excluded spans for the displayed band (default unchanged: the band with the most moon-free time; the global band picker stays deferred).

**Arbitrary-date planning (US2)**

- **FR-008**: The planner MUST let the user choose the planning date (not only "tonight"); all observability outputs MUST be computed for that chosen date and the active site. The date MUST default to "tonight" on each launch and is not persisted across sessions.
- **FR-009** *(amended, iteration 2026-07-17)*: The system MUST compute, for each target, a **best imaging date** — the date on which the target transits at local midnight (anti-solar; best-imaging sense, NOT planetary opposition, and with no magnitude/size change attached) — and present it as a date with a relative "in N days", sortable by days-until. *(Iteration 2026-07-17 — Moon-aware detail best date)*: in the target **detail panel only**, the presented best date MUST additionally avoid the Moon: it is the nearest night to the transit-at-midnight date, within ±15 nights (ties prefer the earlier night; past nights are never recommended), on which the target's lunar separation meets the shared Moon-avoidance minimum (spec 047's Lorentzian rule and live per-band parameters, consumed per FR-023 — scored v1 against the broadband L band; the scoring band is an explicit input so a passband-aware upgrade per FR-035/FR-036 is a parameter change, not a rework). When the transit night itself qualifies, the date is unchanged; when no night in the window qualifies, the transit date is shown with an explicit "no Moon-favourable night" disclosure — never silently. The detail panel MUST explain the shown night's Moon context (illumination and separation) and, when diverged, the skipped transit night's too. The planner **list column ("Opposition") remains the pure geometric transit-at-midnight date** — its value and soonest-next sort are unchanged (see spec 047 FR-014). *Known limitation (accepted)*: each candidate night is scored from a single geocentric Moon snapshot at the candidate instant — a close Moon counts as interfering even when it is below the local horizon that night (the same simplification as the shipped Track-A tonight guidance).
- **FR-010**: Observability computation MUST be parameterized by (site, date); the same target MUST yield different, date-appropriate results as the planning date changes.

**Observing sites (US3)**

- **FR-011**: The system MUST let the user create, edit, and delete named observing sites, each with a name, latitude, longitude, elevation, timezone, twilight definition, and minimum horizon altitude, persisted alongside the existing application settings. Site entry MUST be fully offline: coordinates and elevation are entered manually and the timezone is chosen from a bundled list of IANA zones (no online geocoding).
- **FR-012**: The system MUST let the user mark exactly one site as the default and choose which site is currently active for planning; all planner observability MUST follow the active site, and the active selection MUST persist across relaunches.
- **FR-013**: The system MUST keep the default and active selections valid across site edits and deletions (deleting the active or default site MUST leave a valid selection or the clear no-site state of US6).
- **FR-014**: The system MAY offer to seed a site from an observer location extracted from imported session metadata (latitude/longitude/timezone), with the user confirming and completing it (e.g. elevation) before it is saved — never silently adopted.

**Night-darkness & horizon definition (US4)**

- **FR-015**: The system MUST bound the observable night by a **per-site** twilight definition, defaulting to astronomical twilight (sun 18° below the horizon), switchable to nautical twilight (sun 12° below the horizon).
- **FR-016**: The chosen twilight definition MUST determine the night window used for imaging-time (total and per-band) and the altitude graph's night shading.
- **FR-017**: When no qualifying dark window exists for a site and date under the chosen twilight definition, the system MUST report the absence of a dark window rather than fabricating one; total and per-band imaging time MUST be zero.
- **FR-018**: Each site MUST carry a **minimum horizon altitude** (default 0°) for local obstructions; rise/set, visibility, usable time, and Moon-up determinations MUST respect it, and standard atmospheric refraction MUST be applied at the true horizon.

**Moon geometry & per-filter moon-free time (US5)**

- **FR-019**: The system MUST compute, over the planned night for the active site, the **Moon's altitude** at each sample and the **target-to-Moon angular separation** at each sample, treating the Moon as the single moving body in scope.
- **FR-020**: The system MUST expose, per target, three separation figures — at the target's transit, the minimum over the dark window, and at the dark-window midpoint — presenting "Moon not up" where the Moon is below the horizon at the reference time.
- **FR-021**: The system MUST expose, for the planned night, the **Moon-up windows** (contiguous intervals the Moon is above the site's horizon) intersected with the dark window, and the dark window itself, as raw geometry.
- **FR-022**: The system MUST derive, per target and per band, a **moon-free imaging time** = the summed dark-window intervals where the target is above the usable altitude AND NOT (Moon above the horizon AND target-to-Moon separation below that band's Moon-avoidance minimum for the night's Moon age). The per-band minimum comes from Track A's Moon-avoidance rule; Track B performs the integration over its geometry.
- **FR-023**: The **Moon-avoidance rule** (per-band Lorentzian) and its per-band `(distance, width)` parameters are owned by Track A (spec 047) as a shared module and shared settings; Track B MUST consume that rule/those parameters but MUST NOT define the per-band tolerances, the per-band viability pills, or the filter recommendation.

**Site-first & setup wizard (US6)**

- **FR-024**: The planner MUST render no astronomy and show a clear "add an observing site" prompt whenever no observing site exists, rather than fabricating or blanking values.
- **FR-025**: The first-run setup wizard MUST include a step that creates a default (and active) observing site — name, latitude, longitude, timezone required; elevation optional — and MAY prefill latitude/longitude/timezone from imported session observer metadata for the user to confirm. (This wizard step must be coordinated with any other wizard additions so the wizard is not edited in conflicting ways.)

**Accuracy & scope discipline (cross-cutting)**

- **FR-026**: All computed positions and times MUST be **planning-grade**: approximately 1 arcminute in altitude and approximately ±1 minute for rise, set, and transit. The engine MUST NOT claim or attempt pointing-grade precision. Stored target coordinates (J2000) MUST be corrected to the date of observation before horizontal-coordinate computation.
- **FR-027**: The engine MUST operate fully locally (no online ephemeris service or network dependency) for all in-scope computations, consistent with local-first custody.
- **FR-028**: Target scope MUST be limited to deep-sky (fixed RA/Dec) targets and the Moon; the engine MUST NOT compute planets, comets, asteroids, or other moving bodies.

**Observability presentation iterate (2026-07-15 — decision record: `docs/research/044-047-planner-observability-ux-iterate.md`, PR #819)**

- **FR-029** *(D2)*: Whenever imaging time is 0 for a target/site/date, the UI MUST state the binding blocker rather than showing an unexplained zero: "no astronomical darkness tonight" (dark window null/empty, FR-017) vs "never above `<N>`°" (uptime window empty) vs "Moon too close (`<band>`)" (dark ∩ uptime non-empty but every band's moon-viable window empty). When multiple blockers hold simultaneously, precedence is **darkness > altitude > moon** (report the most upstream structural blocker).
- **FR-030** *(D3-FR1)*: In the planner table's imaging-time cell, a **zero** value MUST show a warning glyph with a reason tooltip — ☀ (darkness), ▲ (altitude), or ☾ (moon) — the same three reasons and precedence as FR-029.
- **FR-031** *(D3-FR2, resolved Q1)*: A **non-zero** imaging-time cell MUST show the ☾ glyph (muted) only when the Moon is the *actionable* binding limiter — some band's moon-viable window is strictly smaller than dark ∩ uptime for that target/night. The trigger is "any band affected" (no per-user band setting); the tooltip MUST name the affected bands, and the per-band truth lives in the detail panel (FR-005). When imaging time is capped purely by darkness/altitude geometry, NO glyph is shown.
- **FR-032** *(D4-FR1/FR2)*: Planner astronomy columns MUST be sized to their real content — the widest real value renders without clipping (fixes #792's stub-width Opposition column). The imaging-time column MUST hold a value like "2h10m" plus the FR-030/FR-031 glyph without clipping. The surviving column set (FR-007) MUST fit without clipping at 1100×720.
- **FR-033** *(D5, resolved Q5)*: The planner table's header/toolbar MUST show, in one always-visible compact single-line place, the active computation context: "Computed for: `<site name>` `<lat>`°N · `<twilight definition>` · ≥`<N>`° · change" — disclosing the active site (FR-012), the twilight definition (FR-015), and the minimum-horizon/usable-altitude value (FR-018) — with "change" opening the existing site/settings switching surface.
- **FR-034** *(D6-FR2 — the #817 fix)*: When no dark window exists for the site/date, the detail altitude graph MUST NOT render the usable-altitude fill as if the night were dark — it MUST either shade the entire plot as non-dark or grey the usable-altitude fill, so the graph agrees with the 0-hour imaging-time stat instead of contradicting it.
- **FR-035** *(D7-FR1)*: The equipment Camera model MUST gain a **sensor-type** dimension: `sensorType: 'mono' | 'osc'`, and for `osc` a **passband**: `'rgb'` (plain color camera) or a narrowband set (dual/tri-band filter, e.g. Ha+OIII). Exactly these fields; no wider equipment-model redesign.
- **FR-036** *(D7-FR2/FR3, resolved Q4)*: For **mono** cameras, per-filter moon-free windows are unchanged (FR-022). For **OSC single-pass** imaging, imaging time MUST collapse to **one window**: the intersection using the strictest (largest) required Moon separation across the passband's bands — `effective_min_sep(age) = max over band in passband of minSeparationDeg(band, age, params)` — reusing Track A's per-band rule verbatim (FR-023) before the existing dark ∩ uptime ∩ moon-viable integration. No new parameter store; explicitly NOT the retired `min_lunar_separation_deg` scalar.
- **FR-037** *(D7-FR5, resolved Q4)*: For an OSC narrowband passband, the detail panel MUST additionally list each captured line's own moon-viable window (e.g. "Ha 4h · OIII 1h"); the strict single-pass number (FR-036) remains the table headline and sort key.
- **FR-038** *(D7-FR4 + defaults)*: When equipment is unset, sensor type MAY be inferred from ingested per-frame FILTER-keyword presence; unknown MUST behave as mono/per-filter (today's behavior), so the change is additive and never regresses mono users.
- **FR-039**: The reason-for-zero text, glyph tooltips, and the computation-context label MUST follow the product's existing localization approach and remain keyboard/screen-reader accessible (glyphs carry text alternatives).

### Key Entities *(include if feature involves data)*

- **Observing site**: A named location — name, latitude, longitude, elevation, timezone (IANA), twilight definition (astronomical/nautical), and minimum horizon altitude. One site is default; one is active for planning. New persisted entity extending the existing settings store; entered manually and offline; a first one is created in the setup wizard.
- **Usable-altitude threshold**: A global user setting (default 30°) — the lowest altitude at which a target is worth imaging. Replaces the hardcoded `USABLE_ALT_DEG`.
- **Moon-avoidance rule + per-band parameters**: Owned by Track A (047); the per-band Lorentzian `(distance, width)` tolerances consumed by this engine to integrate per-band moon-free time. Not defined here.
- **Night observability (per target, per site, per date)**: The position/time result set — altitude samples, transit, rise, set, best imaging date, Moon altitude over time, target-to-Moon separation over time, Moon-up windows, and dark window — from which derived values come.
- **Derived observability (per target)**: Maximum altitude, visible-tonight, band-free total imaging time, the three separation figures, and the per-band moon-free imaging time.
- **Planning context**: The single, shared (active site, planning date, per-site twilight, minimum horizon, usable-altitude threshold, chosen display band) under which the planner and target detail both compute; active site persists, date defaults to tonight. The displayed-band default for the detail graph's Moon overlay is unchanged (band with the most moon-free time, FR-007).
- **Camera sensor type (iteration 2026-07-15)**: The equipment Camera entity gains `sensorType: 'mono' | 'osc'` and, for `osc`, a passband (`'rgb'` or a narrowband set). Consumed by FR-036/FR-037/FR-038; absent/unknown behaves as mono.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For a configured site and date, each in-scope target's maximum altitude and transit time match an independent reference ephemeris to within planning-grade accuracy (≈1 arcmin altitude, ≈±1 min transit) in 100% of sampled cases, with J2000→date correction applied.
- **SC-002**: Rise and set times for targets that have them match an independent reference to within ≈±1 minute (accounting for the site's minimum horizon and refraction); circumpolar and never-rising targets are correctly reported as having none, with zero errors.
- **SC-003**: Changing the usable-altitude threshold updates the total-imaging-time and visible-tonight columns for all targets without any recomputation of positions and without perceptible delay.
- **SC-004**: Selecting a different planning date changes every target's observability to that date's values (distinct from tonight's) in 100% of cases, matching the reference ephemeris for the chosen date.
- **SC-005**: Switching the active observing site recomputes all planner observability for the new site's coordinates in 100% of cases; numbers differ appropriately between two sites with materially different latitude/longitude, and the active site is remembered after relaunch.
- **SC-006**: A user can create, edit, delete, set-default, and activate observing sites entirely offline (manual coordinates + IANA-timezone pick), and the default/active selection remains valid after any such change in 100% of operations.
- **SC-007**: Switching a site between astronomical and nautical twilight changes the night window and the resulting imaging-time figures consistently with the chosen sun-depression angle; raising the minimum horizon reduces a low target's usable time accordingly.
- **SC-008**: At a high-latitude site/date with no dark window under the chosen twilight, the planner reports no dark window and zero total/per-band imaging time rather than fabricating a window, with no error.
- **SC-009**: The three target-to-Moon separation figures match an independent reference ephemeris to planning-grade accuracy; a target adjacent to the Moon reads a small separation and one across the sky reads a large one.
- **SC-010**: Per-band moon-free imaging time equals, for each band, the summed dark-window intervals where the target is above the usable altitude and NOT (Moon above the horizon AND separation below that band's Moon-avoidance minimum for the night's Moon age), in 100% of sampled cases; a more Moon-tolerant band never reports less moon-free time than a stricter band for the same target/night.
- **SC-011**: On first run, completing the wizard's observing-site step yields a persisted default+active site and immediate real planner observability; with no site configured, the planner shows the add-a-site prompt and no astronomy, in 100% of cases.
- **SC-012**: Rise/set/transit and the night window are reported correctly in the site's local time across a daylight-saving transition on an arbitrary planned date.
- **SC-013**: The Moon geometry (altitude-over-time, separation-over-time, Moon-up windows, dark window) and Track A's shared Moon-avoidance rule are computed once and shared, with no duplicate Moon-geometry computation between the tracks.
- **SC-014** *(iteration 2026-07-15)*: The detail altitude graph and the imaging-time stat never contradict: on a no-dark-window site/date (the #817 repro: 52.09°N, 2026-07-14, M31) the graph renders no dark-night usable fill and the stat reads 0 with a stated reason.
- **SC-015** *(iteration 2026-07-15)*: 100% of zero imaging-time cells expose a reason — glyph + tooltip in the table (FR-030), a stated sentence in the detail panel (FR-029); no bare 0 is reachable.
- **SC-016** *(iteration 2026-07-15)*: All surviving planner columns render their widest real values unclipped at 1100×720 (the #792 repro passes for Opposition, and imaging time fits "2h10m" + glyph).
- **SC-017** *(iteration 2026-07-15)*: For a camera configured OSC with a narrowband passband, the table headline equals the strictest-band single-pass window and the detail panel lists each captured line's own window; for mono (or unset) equipment, all values are byte-identical to pre-iteration behavior.
- **SC-018** *(iteration 2026-07-17)*: For a target whose transit-at-midnight date falls near full Moon, the detail-panel best date moves to the nearest Moon-viable night within ±15 nights (earlier preferred on ties) and explains both nights' Moon state; for a Moon-favourable transit night the detail date equals the list's Opposition date; when no night in the window qualifies the transit date is shown with an explicit disclosure. The list column's value and sort are byte-identical to pre-iteration behavior in 100% of cases.

## Out of Scope / Non-Goals

- **Planets, comets, asteroids, and other moving bodies** — only deep-sky fixed targets and the Moon are in scope (FR-028).
- **Planetary opposition** — "best date" is best-imaging date (transit at local midnight / anti-solar), not planetary opposition, and carries no magnitude/size change for a DSO (FR-009).
- **Pointing-grade astrometry** — no nutation/aberration/proper-motion correction beyond the J2000→date correction needed for planning-grade horizontal coordinates; pointing is the mount's / PixInsight's job (FR-026).
- **Online ephemeris / geocoding / catalog services** — all computation and site entry are local (FR-027, FR-011).
- **The Moon-avoidance rule definition, per-band `(distance, width)` parameters, per-band viability pills, filter recommendation, and Moon phase / illumination** — all owned by Track A (spec 047). Track B integrates the rule over its geometry (FR-022/FR-023) but does not define it.
- **The shared astronomy-math implementation choice** and the JS-frontend/Rust-core compute-boundary rationale — plan-stage concerns (the latter recorded as an ADR at plan time and referenced here).
- **Target coordinate resolution / catalog lookup** — supplied by existing target-identity features; this engine consumes coordinates, it does not resolve them.
- **Broader first-run wizard redesign** — this spec adds only the observing-site step/hook; other wizard steps are out of scope and must be coordinated (see Dependencies).

## Assumptions

- **Observing site is a new persisted entity**: The existing settings store gains a named-site collection (name, lat, lon, elevation, timezone, twilight definition, minimum horizon) with a default and an active selection; the existing per-session `ObserverLocation` (tz, lat, lon) remains the acquisition-time record and MAY seed a site.
- **Instant threshold updates**: Positions/times are computed once per (site, date); total imaging time and visible-tonight derive against the current usable-altitude threshold, and per-band moon-free time derives against Track A's per-band parameters — all without recomputing positions. Where the derivation executes is a plan concern.
- **Shared Moon-avoidance rule**: Track A (047) exposes the per-band Lorentzian rule and parameters as a shared module/settings; Track B integrates it. The default per-band parameters and the single-knob retirement are Track A's to set.
- **Planning-grade, not pointing-grade**: ≈1 arcmin altitude and ≈±1 min times suffice; the engine applies the J2000→date correction required for that grade and omits pointing-grade corrections (constitution III — the PixInsight boundary).
- **Local-first**: All computation and site entry are offline; no network dependency (constitution I).
- **Target coordinates available**: Targets carry resolved J2000 coordinates from existing target-identity features; targets without coordinates are shown as un-plannable. (Whether the current targets list endpoint actually populates those coordinates is verified at plan time.)
- **Existing planner surfaces consume the outputs**: The planner table columns and the target-detail altitude graph already exist wired to placeholder logic; this feature replaces the placeholder inputs with real engine outputs rather than redesigning those surfaces.

## Dependencies

- Existing settings/configuration store (spec 018) — extended with the observing-site collection, default/active selection, per-site twilight + minimum horizon, and the global usable-altitude threshold.
- Existing acquisition-session `ObserverLocation` model — optional seed for a site (incl. wizard prefill).
- Existing target-identity / coordinate resolution — supplies target J2000 RA/Dec.
- Existing planner table and target-detail altitude graph surfaces — the consumers of the engine's outputs.
- Track A (spec 047) — provides the shared Moon-avoidance rule/parameters this engine integrates, and consumes this engine's Moon geometry; co-develops the one shared astronomy-math capability (internals specified at plan time).
- First-run setup wizard — gains an observing-site step; **must be coordinated with spec 048's wizard hook** so the two do not make conflicting wizard edits (coordinate via the orchestrator).
- Compute-boundary decision record (ADR) — the JS-frontend vs Rust-core placement of astronomy computation (constitution V) is captured as an ADR at plan time and referenced from this spec.
