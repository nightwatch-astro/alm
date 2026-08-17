# Feature Specification: Typography Rework — Crisp, Consistent, Scalable Text

**Feature Branch**: `055-typography-rework`

**Created**: 2026-07-17

**Status**: Implemented
and task generation

**Input**: User report: "the app's text looks jagged and hard to read on
Windows, and font usage feels inconsistent; also evaluate a larger default and
whole-app zoom like VS Code."

> **Research base.** All load-bearing findings were produced by a completed
> five-lane investigation (live rendering probe on the user's Windows machine,
> repo-wide typography audit, cross-app industry research, engine-zoom
> evaluation, font-selection deep dive with binary probes of candidate font
> files). Key receipts are inlined below; the plan references them per phase.

## Overview

PlateVault's UI text renders soft and jagged on low-DPI Windows displays. The
causal chain, established by a live probe (DPR 1.5, dark theme) and confirmed
against upstream font documentation:

1. **Unhinted Inter.** `tokens.css` loads Inter from the Google Fonts CDN,
   which serves statics instanced from the variable source **with hinting
   stripped** (google/fonts#7007). Below ~150 DPI, TrueType grid-fitting still
   decides stem crispness; unhinted Inter goes mushy exactly in the app's
   dominant size band.
2. **Very small text.** The most common size on data-heavy pages is 12px, with
   48 elements at 10px on Targets; Inter's own docs warn about sub-12px
   rendering on low-DPI.
3. **Fractional pixel arithmetic.** The font-size setting multiplies every
   token by 0.9/1.0/1.15 and writes fractional px (e.g. `11.70px`) onto
   `<html>`; three sidebar labels are literally `9.5px`.
4. **Odd sizes at DPR 1.5.** Odd CSS px sizes land on half device pixels
   (13px → 19.5 device px); nothing can pixel-snap those.
5. **Synthetic styles.** No italic face is loaded — every `em`/`i` and
   `font-style: italic` rule renders faux-oblique. `<strong>` resolves browser
   default 700 against a font loaded only at 400/500/600.
6. **Offline nondeterminism.** A local-first desktop app fetches its UI font
   from a CDN on every launch; offline, rendering silently changes. JetBrains
   Mono is also downloaded on every launch but is dead weight — a global
   `* { font-family: … !important }` reset strips every monospace surface.

This feature fixes all six in one coherent rework: bundled hinted fonts, a
rem-based token layer with an integer-pixel size dial, a raised size floor, an
explicit semantic-element base layer, deliberate monospace restoration, and
VS Code-style whole-app zoom alongside the font-size dial.

## User Decisions (recorded 2026-07-17)

- **Size dial**: Small/Default/Large = **12/14/16px** root (default bumps the
  base 13→14).
- **Zoom**: evaluate VS Code-style app zoom — verdict adopted: **add engine
  zoom alongside the dial, do not replace the dial** (VS Code ships exactly
  this split).
- **Monospace restoration scope**: filesystem paths, source-view IDs, manifest
  paths, dev-tools contract names, **and RA/Dec coordinates**.

## Functional Requirements

### FR-001 — Bundle hinted Inter statics; delete CDN font loads

Self-host the **six hinted static woff2s from the Inter v4.1 release**
(`extras/woff-hinted/`): Regular, Medium, SemiBold, each in roman + italic
(~858KB total). Remove both Google Fonts CDN imports.

> **Correction to earlier working notes:** "hinted InterVariable" does not
> exist. Binary probe of Inter-4.1.zip: `InterVariable.ttf` has zero hint
> tables; the hinted assets are the static woff2s only (1,965/2,937 glyphs
> instructed). The app uses exactly weights 400/500/600 and never animates
> weight, so statics cost nothing. Do **not** bundle InterVariable or
> InterDisplay.

- `--alm-font-sans` keeps the family name `Inter` — no metric churn.
- Keep `font-feature-settings: 'tnum' 1` (load-bearing: Inter v4 digits are
  proportional by default). Add `'calt' 1`; evaluate `'case' 1` and `'zero' 1`
  (slashed zero) for coordinate/table surfaces during Windows verification.
- Pin the release (v4.1, 2024-11-15), record the release URL + SHA-256 in the
  repo, ship the OFL `LICENSE.txt` alongside the fonts.

### FR-002 — Rem token layer with an integer root dial

- Convert the type-token layer to **rem**; the font-size setting writes a
  single **integer** `html { font-size }` of 12, 14, or 16px. Delete the
  per-token fractional scaler in `theme.ts` (`(px * scale).toFixed(2)`).
- **All three dial positions MUST produce sane computed sizes**: no computed
  font-size below the floor (FR-003), and no fractional CSS px from the token
  layer at any dial stop (per-token rounding is the fallback mechanism if the
  ratio set cannot land integers at all stops).
- Re-derive the scale from the 14px base; top end ~24px; prefer even px values
  (integer device pixels at DPR 1.0/1.5/2.0).

### FR-003 — Raise the size floor; retire micro-sizes

- Minimum rendered text size is **11px** at the default dial stop. Retire the
  10px token; replace the 9/9.5px hardcodes.
- Tokenize the **11 hardcoded px font-sizes** currently immune to the size
  setting (sidebar/settings group labels, wizard titles and fine print, planner
  SVG axis text, toast glyph).
- Consolidate uppercase micro-labels onto one token (today: three sizes for
  the same visual role) and introduce a letter-spacing token set (today: 8
  divergent untokenized values across 13 files).

### FR-004 — Semantic-element base layer

Every semantic text element gets an explicit token-based rule; nothing falls
through to browser defaults:

- `strong, b { font-weight: var(--alm-weight-semibold) }` — never 700, never
  synthetic bold.
- `em, i` render with the **real italic faces** loaded by FR-001 — synthetic
  italics are eliminated app-wide.
- `code, pre, kbd` render in the monospace stack (FR-005) — a deliberate
  visual change from today's Inter-forced rendering.
- Remove or define the dead `font-mono` class (currently a silent no-op).
- The global `* { font-family: … !important }` blanket is replaced by the base
  layer + explicit stacks, so intentional deviations become possible and
  auditable.

### FR-005 — Monospace restoration

Restore monospace on: filesystem paths, source-view IDs, manifest paths,
dev-tools contract names, `code`/`pre` content, and **RA/Dec coordinate
values**. The stripped-then-commented mono intents in the stylesheets are
re-added deliberately (they do not resurface automatically).

Default face: **system mono stack** (`ui-monospace, 'Cascadia Code',
monospace`) — natively hinted on every OS at zero bytes. If Windows
verification shows unacceptable cross-OS inconsistency, fall back to bundling
JetBrains Mono woff2 (same pin-and-license discipline as FR-001). Either way
the current CDN JetBrains Mono load is deleted with FR-001.

### FR-006 — Whole-app engine zoom (alongside the dial)

- Add Tauri v2 `setZoom` engine zoom: true layout zoom on WebView2 (ZoomFactor)
  / WKWebView (pageZoom) / WebKitGTK. CSS `zoom` is explicitly rejected (it
  contaminates viewport measurement and creates fractional font sizes).
- App-owned shortcuts **Ctrl+= / Ctrl+− / Ctrl+0** (WebView2 exposes no
  zoom-change event; owning the write path is the only reliable persistence).
  Persist like the existing font-size setting.
- Capability: `core:webview:allow-set-webview-zoom`.
- **Sequencing constraint**: at 150% zoom the 1100×720 minimum window yields a
  733px CSS viewport — below every layout floor. Zoom ships **after spec 054
  (adaptive detail dock)** absorbs narrow viewports, or ships earlier capped at
  **125%**.
- Spec 054 clarification (one line, to be added there): its thresholds are
  CSS-px viewport measurements, never Tauri window size.

## Success Criteria

- **SC-001**: Zero network font requests at runtime (assertable in E2E: no
  requests to `fonts.googleapis.com`/`fonts.gstatic.com`); rendering is
  byte-identical offline.
- **SC-002**: Computed `font-family` on body text resolves to the bundled
  Inter; `document.fonts` reports the six faces loaded, including italics.
- **SC-003**: At every dial stop (12/14/16), `html` computed font-size is an
  integer and no element's computed font-size is fractional or below 11px
  (CI-assertable via computed-style sweep).
- **SC-004**: The font-size setting scales **all** text — the 11 previously
  hardcoded sizes included.
- **SC-005**: No synthetic bold or synthetic italic: every rendered
  weight/style maps to a loaded face.
- **SC-006**: Mono surfaces (paths, IDs, contract names, RA/Dec values) render
  in the monospace stack.
- **SC-007**: With zoom shipped: zoom persists across restart, Ctrl+0 resets,
  and a CI pin passes at minimum window × maximum zoom (layout does not break;
  with 054: dock falls back to bottom mode).
- **SC-008**: Windows real-app A/B verification (hinted vs current CDN build,
  DPR 1.5, dark theme) confirms the crispness improvement before the token
  rework lands — the font swap is the first, independently shippable step.

## Verification Obligations (standing rules)

- **CI UI assertions**: SC-001/002/003/007 are enforced as automated E2E
  assertions, not manual checks.
- **Journey delta**: the wave ships a **J10 (ingestion settings / appearance)
  journey delta** covering the new dial semantics and zoom, per the standing
  journey-deltas-per-wave rule.
- **Windows verification**: a `verify-on-windows` scenario accompanies the
  font-swap phase (SC-008) and the zoom phase.

## Out of Scope

- Font personality/brand exploration beyond the completed deep dive (Inter won
  on evidence; IBM Plex Sans Var was the only near-contender and loses on
  full-app migration cost for zero rendering gain).
- Per-monitor/per-DPR token tables ("round to device pixels") — rejected;
  hinting is the correct mechanism, and even-px preference captures the rest.
- Spec 054's dock behavior itself (only the one-line CSS-px clarification).
