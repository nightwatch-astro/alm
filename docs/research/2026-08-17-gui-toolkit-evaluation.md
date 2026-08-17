# PlateVault UI toolkit assessment: Tauri vs Rust-native GUI

Date: 2026-08-17. Evidence base: `origin/main` at fetch time; crates.io API; GitHub API; upstream READMEs/docs.

Verdict: **stay on Tauri.** Two facts decide it, both verified below: no Rust-native
toolkit currently meets the stated WCAG-AA/screen-reader bar with a shipped, tested
implementation, and the migration would discard ~90k lines of non-test TS/TSX plus
~55k lines of frontend tests. A third fact makes the question moot as a coupling
problem: the core is already contract-separated, so Tauri is a replaceable shell,
not an architectural commitment.

---

## 1. Verified requirements

### Accessibility (decisive)

`PRODUCT.md` "Accessibility & Inclusion": "Target WCAG AA. Support keyboard-first
navigation for review workflows, visible focus states, reduced-motion operation,
semantic status/error messaging, and clear differentiation that does not rely on
color alone." This is a product-level commitment, not a nice-to-have.

The frontend already implements against it: 541 `aria-*` attribute occurrences in
non-test TS/TSX (`rg` count over `apps/desktop/src`), `eslint-plugin-jsx-a11y@6.10.2`
in `apps/desktop/package.json`, a contrast checker at
`apps/desktop/scripts/check-contrast.mjs`, and a dedicated journey
`docs/journeys/J16-keyboard-first-navigation/journey.md` ("Drive PlateVault end to
end without a pointer", version 4, last_reviewed 2026-07-14).

Note: WCAG AA is a *web* conformance standard. It is authored against a DOM
accessibility tree. A native toolkit can satisfy the intent (screen reader, focus,
keyboard) but cannot be audited against WCAG the same way, and every existing
axe/jsx-a11y check would become unrunnable. That is a requirements regression, not
just a port.

### Shell capabilities in use

From `apps/desktop/src-tauri/Cargo.toml` and the registrations in
`apps/desktop/src-tauri/src/lib.rs` (`plugin(` at lines 125, 166, 193, 194, 202,
203, 217, 228, 239):

| Capability | Plugin | Registration |
| --- | --- | --- |
| Single instance (guards canonical SQLite) | `tauri-plugin-single-instance` | lib.rs:125, gated by `bootstrap::single_instance_guard_enabled` |
| Window geometry persistence | `tauri-plugin-window-state` | lib.rs:166 |
| Native file/folder pickers | `tauri-plugin-dialog` | lib.rs:193 |
| Reveal in OS / open URL | `tauri-plugin-opener` | lib.rs:194 |
| Signed auto-update | `tauri-plugin-updater` | lib.rs:202 via `build_updater_plugin()` |
| Restart/exit | `tauri-plugin-process` | lib.rs:203 |
| Rotating file log | `tauri-plugin-log` | lib.rs:217 |
| MCP bridge (dev) | `tauri-plugin-mcp-bridge` | lib.rs:228 |
| WebDriver server (e2e feature) | `tauri-plugin-webdriver` 0.2 | lib.rs:239 |

Corrections to the brief's assumed list: `tauri-plugin-notification` is **not** a
dependency (audit `docs/research/tauri-plugin-api-audit-2026-07-05.md` recommends it
as an unadopted item, ranked #5). There is no native-menu plugin. Two plugins the
brief omitted are present: `mcp-bridge` and `webdriver`.

Updater is real, not aspirational: `apps/desktop/src-tauri/tauri.conf.json` carries a
minisign `pubkey`, `createUpdaterArtifacts: true`, and a GitHub releases endpoint.

Splash/window handshake: main window ships `"visible": false` and the splash owns the
reveal after a boot-ready handshake (lib.rs:162-165 comments).

### Frontend surface (what a rewrite discards)

Measured by extracting `apps/desktop/src` + `apps/desktop/tokens` from `origin/main`:

| Metric | Count |
| --- | --- |
| Files under `apps/desktop/src` | 744 |
| `.tsx` files | 397 |
| `.ts` files | 300 |
| Test/spec files | 271 |
| Total TS/TSX lines | 145,704 |
| — non-test | 90,419 |
| — test | 55,285 |
| `.css.ts` (vanilla-extract) files / lines | 28 / 2,071 |
| Feature components (`features/**/*.tsx`, non-test) | 144 |
| Shared components (`components/`) | 24 |
| UI primitives (`ui/`) | 21 |
| Design-token JSON files | ~19 incl. 6 themes, 2 density modes, 6 component groups |
| i18n messages (`messages/en-GB.json`) | 2,254 keys |
| Locales (`project.inlang/settings.json`) | 2 — `en-GB`, `pt-BR` |
| Playwright specs (`tests/e2e/*.spec.ts`) | 35 |
| Rust e2e journeys (`crates/e2e-tests/tests/`) | 16 test files |
| Journey docs (`docs/journeys/`) | J01–J16+ |

The vanilla-extract token pipeline (`style-dictionary@5.5.0` → `tokens/*.json` →
`.css.ts`) is recent sunk cost, plus `size-limit` bundle guards
(`specs/028-frontend-quality-hardening/tasks.md:120-126`).

### Validation harness

Two layers, both webview-dependent:

- Layer 1: Playwright, 35 specs, mock-mode frontend. `tests/e2e/playwright.config.ts`
  documents a dedicated e2e port because concurrent worktrees collide on 5173.
- Layer 2: `crates/e2e-tests` — thirtyfour W3C client → `tauri-webdriver` CLI on :4444
  → embedded `tauri-plugin-webdriver` on :4445, real backend, real SQLite
  (`crates/e2e-tests/README.md`).

A native toolkit destroys both. Replacements exist (`egui_kittest` 0.36.1,
`iced_test` 0.14.0, `freya-testing` 0.4.1, `masonry-testing` 0.4.0) but they are
in-process widget harnesses, not out-of-process app drivers. There is no
`driver.click(selector)` equivalent that exercises the real shipped binary.

### Backend scale (the counterweight)

236 files under `crates/app`, 473 crate source files overall, 45 files in
`packages/contracts`. Constitution Principle V already mandates language-neutral
contracts with Tauri as "the first adapter". The UI is one adapter of a large,
already-portable core — which is exactly why swapping it buys less than it appears to.

---

## 2. Candidate assessment

All versions/dates from the crates.io API and GitHub API on 2026-08-17.

### Not peers (stated as the brief asked)

- **masonry** 0.4.0 (2025-10-29, Apache-2.0) is Xilem's widget layer, published from
  the same repo `linebender/xilem`. Its own README: "Masonry is a toolkit for
  building UI frameworks (including Xilem)". Scoring it separately would double-count.
- **crux** (`crux_core` 0.20.0, 2026-08-07) is a core/shell architecture pattern with
  no renderer. It is a *shape* PlateVault already substantially has via
  `packages/contracts` + `crates/app`. It is assessed in §4 as a middle option, not
  as a UI candidate.

### iced 0.14.0 — 2025-12-07, MIT, 31.3k stars

Wide widget inventory including `table.rs`, `pane_grid`, `combo_box`, `text_editor`,
`lazy`/`keyed` (virtualisation primitives) — 49 modules in `widget/src`.

**Accessibility: none shipped.** Verified: `iced 0.14.0` and `iced_winit 0.14.0` have
zero `accesskit*` dependencies (crates.io dependency API). `iced-rs/iced#552`
"Implement accessibility support" — open since 2020-10-05, last activity 2026-02-04,
26 comments, still discussing whether AccessKit is the right start. PR #3111
"draft: Accesskit integration" and #1849 "WIP: Iced accessibility" both open,
last touched 2026-05-22. #489 (open, 2026-05-24): "In native UI, cannot focus most
widgets, control them via keyboard, or tab between widgets."

**USP for PlateVault:** the strongest table + pane-splitting widget set of any
candidate, an Elm-style message architecture that maps cleanly onto the existing
contract call/response shape, and `iced_test` for deterministic UI assertions.

**Verdict: NOT-VIABLE.** Cannot tab between widgets. Fails the keyboard-first
requirement at the framework level, not the app level.

### egui 0.36.1 — 2026-08-07, MIT OR Apache-2.0, 30.1k stars

Fastest cadence of any candidate (0.34.1 → 0.36.1 across 2026-03 to 2026-08).
**Non-optional `accesskit ^0.24.1` dependency** — the only mature candidate where a11y
is always compiled in, verified via crates.io deps (`optional: false`).

Gaps: a11y is not on by default in the demo (emilk/egui#2960 open, 2026-03-04); live
regions unimplemented (#2647 open since 2022-02-08) — that directly hits PRODUCT.md's
"semantic status/error messaging". Immediate-mode retained state is a poor fit for
144 stateful feature components with forms, wizards, and dialogs. No native menus,
no text selection parity, and re-laying-out every frame is a battery/GPU cost on a
desk app that is mostly static tables.

**USP for PlateVault:** unmatched release cadence; a11y always-on rather than
opt-in; `egui_kittest` gives snapshot-tested UI in-process; trivially embeds
custom-drawn views (sky charts, coverage bars).

**Verdict: VIABLE-WITH-GAPS** (missing live regions; immediate-mode misfit for
form-heavy screens; no native menu).

### xilem 0.4.0 / masonry 0.4.0 — 2025-10-29, Apache-2.0, 5.5k stars

AccessKit is an explicit architectural pillar (README: "AccessKit for plugging into
accessibility APIs"); `masonry 0.4.0` depends on `accesskit ^0.21.1`. Widget set is
respectable for its age — 38 modules including `virtual_scroll.rs`, `grid.rs`,
`split.rs`, `text_input.rs`, `radio_group.rs`, `collapse_panel.rs`.

Killers: 9,795 lifetime downloads for `xilem` (vs 2.5M for iced, 21.6M for egui) —
effectively no production user base to have found the bugs. Last release 2025-10-29,
nearly 10 months before this assessment, on a 0.x line. No `table` widget. AccessKit
pinned two majors behind current (0.21 vs 0.24.1).

**USP for PlateVault:** the only candidate with a first-class retained widget tree
*and* AccessKit designed in from the start, plus `virtual_scroll` built in and the
Vello/Parley text stack (best text shaping of the native set — relevant for the
typography rework, spec 055).

**Verdict: VIABLE-WITH-GAPS** — technically the best-architected fit, but adopting a
framework with 9.8k downloads for a product heading to release is taking on
upstream maintenance you did not budget for.

### dioxus 0.7.10 stable / 0.8.0-alpha.1 — 2026-07-31, MIT OR Apache-2.0, 38.8k stars

Highest star count; strong cadence. But `dioxus-desktop 0.7.10` depends on `wry` —
**it is a webview**, i.e. the same WebView2/WebKitGTK substrate as Tauri. Migrating
to Dioxus desktop does not escape any webview problem; it trades React for RSX and
loses the plugin ecosystem. Its native path is `blitz` (`blitz-dom 0.3.0-beta.1`,
stable 0.2.4, AccessKit ^0.17 — three majors behind), which the Dioxus README itself
labels "Experimental Native Renderer".

**USP for PlateVault:** the only candidate offering an incremental, component-by-
component port path from React (same reactive model, HTML/CSS styling, so the
vanilla-extract tokens have a survivable analogue) with a future native escape hatch.

**Verdict: NOT-VIABLE as a native migration** (webview-backed today, experimental
native renderer). Would be VIABLE-WITH-GAPS purely as a React replacement — but that
is a different, lower-value project.

### slint 1.17.1 — 2026-07-07, 23.5k stars

The only **1.x** candidate: real semver stability guarantees, an `accessibility`
cargo feature, `accesskit` + `accesskit_winit` in `i-slint-backend-winit 1.17.1`
(verified via crates.io deps), and the richest documented a11y vocabulary of any
candidate — 21 `accessible-*` properties, 21 `AccessibleRole` values including
`table`, `list-item`, `radio-group`, plus ARIA-style landmark roles and
`accessible-live` (`off`/`polite`/`assertive`) for status announcements. That last
one is the exact PRODUCT.md "semantic status/error messaging" requirement, and no
other candidate documents it. `tree` role documented as not yet provided.

**Licence problem.** Crate licence is
`GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`.
PlateVault is AGPL-3.0-only (SPDX headers throughout, e.g. `apps/desktop/src/lib/i18n.ts`).
GPL-3.0-only is **not** compatible with AGPL-3.0-only in the direction needed: you
cannot ship AGPL-3.0-only code linked against a GPL-3.0-only library without the
combination being unlicensable, because AGPL-3.0-only lacks the §13 relicensing
grant that AGPL-3.0-or-later would provide. The Royalty-free option is for
*proprietary* apps and Slint requires per-seat licensing for everyone touching
design/dev/test. **This requires a licence-policy decision, not an engineering one.**

Also: a custom DSL (`.slint`) means the entire 90k-line TS surface is rewritten in a
language with no ecosystem, and the DSL is compiled — no hot-swap of business logic.

**USP for PlateVault:** the only 1.x-stable candidate; the only one with documented
live-region support and a `table` accessible role; `system-testing` feature and a
commercial GUI test framework; commercial support available.

**Verdict: VIABLE-WITH-GAPS, blocked on licence.** Technically the closest match to
the a11y requirement of any native candidate. Escalate the AGPL-3.0-only × GPL-3.0-only
question before spending further effort.

### gpui 0.2.2 — 2025-10-22, Apache-2.0, published from zed-industries/zed (88.7k stars)

**Is it consumable standalone?** Partially, and newly so. `gpui` is on crates.io
(`publish = true` in `crates/gpui/Cargo.toml` at Zed HEAD), first published
2025-10-05, 0.2.2 on 2025-10-22 — no release in the ~10 months since. The README at
HEAD documents a public API split into `gpui` + `gpui_platform`, so the published
0.2.2 surface **already differs from HEAD** and the split is not in a release. The
README states plainly: "GPUI is still in active development as we work on the Zed
code editor, and is still pre-1.0. There will often be breaking changes between
versions." That is the deciding practical fact: you would be tracking a monorepo's
internal crate, on an API that changed shape since its last publish, with no
release cadence.

The existence of a third-party `gpui-unofficial 1.15.0` fork chain is itself
evidence the official crate is not comfortably consumable.

**Licence: Apache-2.0** for the `gpui` crate specifically (its own `Cargo.toml`
declares `license = "Apache-2.0"`, distinct from the Zed repo's NOASSERTION mix).
Apache-2.0 is inbound-compatible with AGPL-3.0-only. No licence blocker.

**Accessibility:** better than I expected, and worse than the version numbers admit.
gpui at HEAD has `accesskit.workspace = true` and a documented guide module
`crates/gpui/src/_accessibility.rs`: "GPUI integrates with AccessKit to provide
programmatic accessibility features", with an `examples/a11y` directory and a
`tab_stop.rs` module. **But the published 0.2.2 has zero `accesskit*` dependencies**
(verified via crates.io deps: 95 normal deps, none matching). So a11y exists only at
git HEAD, in an unreleased API, documented as a guide rather than as an audited
widget inventory. Do not extrapolate from Zed's polish: Zed's own a11y is a
long-standing gap, and gpui exposes no accessible widget library — the entire
`AccessibleRole` vocabulary Slint documents has no gpui equivalent.

**Windows/Linux:** HEAD README documents Win32 + DirectWrite on Windows (no features
needed) and Wayland/X11 features on Linux/FreeBSD, so both are real. But there is no
widget library at all — gpui gives you `div()`, taffy flexbox layout, and text.
`crates/gpui/src/platform` contains `app_menu.rs` (native menus exist) but no
combobox, no date picker, no table, no dialog, no tree.

**Do its strengths match this app?** Partly. GPU-rendered very-long virtualised lists
are genuinely relevant to large astro libraries. But PlateVault is settings-heavy,
form-heavy, wizard-heavy, dialog-heavy — 144 feature components and a `WizardShell`
of 8.3 KB. You would hand-build every control from `div()`. That is the opposite of
the widget inventory this app needs.

**Verdict: NOT-VIABLE.** Deciding fact: the published 0.2.2 has no AccessKit at all,
its API has already been restructured at HEAD without a release, and it ships no
widget library for a form-heavy app.

### azul — `azul` 1.0.0-alpha4 (2021-08-05), MIT, 6.1k stars

The git repo is *very* active — `fschutt/azul` pushed 2026-08-17, commits that same
day, and `azul-core 0.0.14` published 2026-08-17 (with `azul-layout 0.0.14` depending
on `accesskit ^0.24`, the current major). So "dormant" is wrong as a repo claim.

But the consumable story is the classic docs/reality gap, and worse than the brief
feared. The `azul` façade crate's newest version is `1.0.0-alpha4` from **2021-08-05**;
`max_stable_version` is `0.1.0` from 2018. `azul-desktop` last published 2020-05-14.
Only the internal `azul-core`/`azul-layout` crates are being released in 2026. And
the README at HEAD says it outright:

> **This repository is currently under heavy development. Azul is NOT usable yet.**
> APIs may change frequently and features may be incomplete or unstable. […]
> The current release is from 2+ years ago.

Its build path also runs a codegen multitool (`azul-doc codegen all`) to produce the
public API — a bespoke pipeline, not `cargo add`.

**Licence: MIT** — inbound-compatible with AGPL-3.0-only. No blocker. (Note the
internal crates relicensed MPL-2.0 → MIT between 0.0.8 (2026-05-23) and 0.0.9
(2026-07-14); both are fine inbound.)

**USP for PlateVault:** an HTML/CSS-like DOM model with a language-neutral generated
C API — in principle the least-alien port target for an existing CSS-styled app, and
the AccessKit version it tracks is current. That is a real technical thesis.

**Verdict: NOT-VIABLE.** Deciding fact: the project's own README at HEAD states it is
not usable, and the consumable façade crate has not been published since 2021. Nothing
about a11y, updaters, or tables needs evaluating past that.

### cushy 0.4.0 — 2024-08-20, MIT OR Apache-2.0, 606 stars

Last publish nearly two years ago; last repo push 2025-09-04; no `accesskit`
dependency found in the AccessKit reverse-dependency list.
**Verdict: NOT-VIABLE** (effectively unmaintained relative to a release timeline; no
a11y). No USP that another candidate does not cover better.

### freya 0.4.1 stable / 0.5.0-rc.3 — 2026-08-16, MIT, 3.0k stars

Active (0.4.0 → 0.5.0-rc.3 in one month). `freya-core 0.4.1` depends on
`accesskit ^0.24.0` — **current major**, matching egui. Has `freya-testing 0.4.1`.
Skia renderer; Dioxus-style RSX components, so React-familiar.

Gaps: 37.7k lifetime downloads, 44 open issues, 3.0k stars — smallest active
community of the viable set. Rapid pre-1.0 churn (an rc every week). No table widget.
No updater/single-instance/window-state story; also ships `freya-webview`, implying
some things still need a webview.

**USP for PlateVault:** the only candidate combining a *current* AccessKit major, a
React-like component model (cheapest mental port from the existing 144 components),
and a first-party testing crate.

**Verdict: VIABLE-WITH-GAPS** (community size and pre-1.0 churn are the risk, not
the a11y story).

---

## 3. Comparison table

| Candidate | Version / date | Licence | Maturity | Accessibility | Required-capability gaps | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| **Tauri (incumbent)** | 2.11.5, 2026-07-01 | MIT/Apache-2.0 | 2.x stable, 26.1M dl | DOM a11y + platform screen readers; 541 aria uses in-repo | — (all 8 capabilities in use) | **BASELINE** |
| iced | 0.14.0, 2025-12-07 | MIT | 0.x, 2.5M dl, 31.3k★ | **None.** #552 open since 2020; #489: cannot tab between widgets | a11y, updater, single-instance, window-state, notifications, native menus, out-of-proc e2e | **NOT-VIABLE** |
| egui | 0.36.1, 2026-08-07 | MIT/Apache-2.0 | 0.x, 21.6M dl, 30.1k★ | AccessKit 0.24.1 **non-optional** | live regions (#2647), native menus, updater, single-instance, window-state, immediate-mode misfit for 144 form components | **VIABLE-WITH-GAPS** |
| xilem (+ masonry) | 0.4.0, 2025-10-29 | Apache-2.0 | 0.x, **9.8k dl**, 5.5k★, 10mo since release | AccessKit 0.21.1 (2 majors behind), designed-in | table widget, updater, single-instance, window-state, notifications, community size | **VIABLE-WITH-GAPS** |
| dioxus | 0.7.10, 2026-07-30 | MIT/Apache-2.0 | 0.x, 2.3M dl, 38.8k★ | via `wry` webview (same as Tauri); native path AccessKit ^0.17 | **is a webview** — solves no webview problem; native renderer "experimental" | **NOT-VIABLE** (as native migration) |
| slint | 1.17.1, 2026-07-07 | **GPL-3.0-only** OR royalty-free OR commercial | **1.x stable**, 1.5M dl, 23.5k★ | Best documented: 21 `accessible-*` props, 21 roles, `accessible-live` | **licence × AGPL-3.0-only**, custom DSL discards 90k LOC of TS, `tree` role absent | **VIABLE-WITH-GAPS, licence-blocked** |
| gpui | 0.2.2, 2025-10-22 | Apache-2.0 | pre-1.0, no release in 10mo; HEAD API ≠ published API | **None in 0.2.2**; AccessKit only at unreleased HEAD | **no widget library** (div + taffy only); no combobox/table/dialog/tree; updater; window-state | **NOT-VIABLE** |
| azul | 1.0.0-alpha4, **2021-08-05** | MIT | README: "Azul is NOT usable yet" | AccessKit ^0.24 in `azul-layout` only | consumable crate 5 years stale; bespoke codegen build; everything else moot | **NOT-VIABLE** |
| cushy | 0.4.0, 2024-08-20 | MIT/Apache-2.0 | 6.5k dl, 606★, repo idle since 2025-09 | none found | all | **NOT-VIABLE** |
| freya | 0.4.1, 2026-08-02 (rc.3 2026-08-16) | MIT | 0.x, 37.7k dl, 3.0k★ | AccessKit **0.24.0** (current) | table widget, updater, single-instance, window-state, community size, weekly rc churn | **VIABLE-WITH-GAPS** |
| *masonry* | 0.4.0 | Apache-2.0 | — | — | **Not a peer** — xilem's widget layer, same repo | n/a |
| *crux* | `crux_core` 0.20.0, 2026-08-07 | Apache-2.0 | 339k dl | — | **Not a renderer** — architecture pattern; see §4 | n/a |

Capability-replacement crates all exist if you did migrate: `rfd 0.17.2` (pickers),
`muda 0.19.3` (menus), `notify-rust 4.18.0`, `tray-icon 0.24.2`,
`accesskit_winit 0.33.2`, `self_update 0.44.0` / `cargo-packager-updater 0.2.3`.
The weak one is `single-instance 0.3.3` — last published **2021-12-16**, and the
canonical-SQLite guard depends on it.

---

## 4. What would migrating actually solve?

### Real, repo-evidenced Tauri pain

There is genuine pain, and it is all one thing: **two divergent webview engines**.

- `docs/development/orchestration-2026-07-06.md` — 14 occurrences of
  WebView2/WebKitGTK. Documented cases: a Tauri IPC error/race in Windows WebView2
  that the debug bridge bypasses (:93); a WebView2 layout/virtualizer race on hard
  refresh (:94); "a green run proves nothing about a fix" (:106); a portal race where
  `Combobox.Portal` gates rendering (:177); WebView2's async localStorage flush lost
  on forced process kill, with **different timing on WebKitGTK** (:212-223); storage
  reset deleting WebView2's EBWebView profile but silently no-op-ing on Linux, so
  "journeys share webview state on ubuntu" (:311-317).
- `docs/memory/BUGS.md:61,67` — WSLg cannot render WebKitGTK, so Tauri windows do not
  work there at all.
- `docs/development/design-review-2026-07-11.md:286` — a WebView2-specific HWND
  constraint the app must respect.

That is the honest case *for* migrating: a single Rust renderer would eliminate an
entire class of platform-divergence bugs and make e2e results mean the same thing on
every OS. It is a real benefit and should not be waved away.

### What migrating would NOT solve

- **IPC and bindings.** The contract boundary is required by Constitution Principle V
  independently of the UI toolkit. `specs/037-ipc-wrapper-removal` and
  `specs/042-stdlib-adoption` (§D "Frontend — type safety & IPC boundary") show this
  is being actively simplified *within* Tauri. A native UI removes serialisation but
  the contract stays — and `packages/contracts` exists precisely to keep a future
  remote backend possible.
- **`dev-tools` feature gating.** Feature-gating a developer surface out of release
  builds is a correctness requirement, not a Tauri artefact. It survives any toolkit.
- **Mock-mode indirection.** Needed to test UI without a backend. Survives any toolkit.
- **e2e port collisions.** Caused by concurrent worktrees sharing a dev-server port
  (`tests/e2e/playwright.config.ts` comment), not by Tauri.
- **Bundle size.** Guarded by `size-limit` and never recorded as a problem.

So of six candidate motivations, exactly one is real, and it is webview divergence.

### The middle options

1. **Reduce IPC surface / keep Tauri.** Already in flight (specs 037, 042). Cheapest,
   already funded.
2. **Rust-native shell + existing web UI.** This is what Tauri already *is*. Building
   your own wry/webview shell to keep the web UI would rebuild eight plugins to
   arrive at the same substrate and the same WebView2/WebKitGTK divergence. No gain.
3. **crux-style core/shell separation.** Constitution V arguably already mandates it
   and `crates/app` + `packages/contracts` largely implement it. The valuable move
   here is *tightening* that boundary — no UI feature reaching past the contract —
   which makes any future toolkit swap a bounded project rather than a rewrite.
   **This is the constructive version of the question.** It costs a fraction of a
   migration and preserves the option.
4. **Narrow the divergence instead of the toolkit.** The specific WebKitGTK/WebView2
   storage and reset asymmetries are addressable directly, and Tauri is moving toward
   a unified Servo/Verso backend upstream (not verified as production-ready here —
   see §7).

---

## 5. Recommendation

**Stay on Tauri. Do not migrate.** Instead spend a fraction of the migration budget
tightening the contract boundary (option 3) so the decision stays reversible, and
address the WebKitGTK/WebView2 storage-reset asymmetry directly, since that is the
one Tauri-caused problem with evidence behind it.

The three facts that drive it:

1. **No candidate meets the a11y requirement with shipped, tested code.** The only
   one whose documented a11y vocabulary actually matches PRODUCT.md — including
   `accessible-live` for status messaging — is Slint, and Slint's GPL-3.0-only option
   is incompatible with an AGPL-3.0-only app. iced cannot tab between widgets.
   gpui's published crate has no AccessKit at all. Every other candidate has AccessKit
   plumbing but no audited accessible widget inventory, and none has an equivalent of
   `eslint-plugin-jsx-a11y` + axe + a contrast checker in CI.
2. **The cost is 145,704 lines of TS/TSX (90,419 non-test), 189 components, 2,254
   i18n keys across 2 locales, ~19 token files with 6 themes, 35 Playwright specs,
   and a WebDriver-based real-binary e2e layer** — with no out-of-process journey
   driver in any native toolkit to replace that last item.
3. **Tauri is already a thin, replaceable adapter.** Contracts (45 files) and
   `crates/app` (236 files) hold the value; the webview holds presentation. The
   architecture the migration would be *for* is largely already there, which means
   migrating buys portability you already own.

Migration cost if you moved anyway: I would not estimate calendar time, but the
discarded artefact set is quantified above, and it excludes the a11y remediation
needed to reach parity on any candidate — which is unbounded, because no candidate
has a shipped equivalent.

## 6. Strongest argument against this recommendation

**The webview divergence is not a bug class you can finish fixing, and it is already
corrupting your validation signal.**

`orchestration-2026-07-06.md:106` records that "a green run proves nothing about a
fix". That is the most serious sentence in the repo for this question: it says the
project's own e2e harness produced results that could not be trusted, because
WebView2 and WebKitGTK behave differently in storage flushing, portal rendering, and
virtualizer layout. The same file documents that a Linux storage reset silently
no-ops, so ubuntu journeys **share state across journeys**. A test suite with
undetected cross-test contamination on one of three target platforms is not a working
suite, and 35 Playwright specs plus 16 Rust journey files are only as valuable as
that trust.

A single Rust renderer makes those bugs structurally impossible. One layout engine,
one text stack, one storage story, identical on all three OSes. On that argument, the
145k lines are not an asset — they are 145k lines written against two engines that
disagree, and their test coverage partly measures webview quirks rather than product
behaviour. Anyone who has spent multiple orchestration cycles chasing WebView2 races
would tell you the sunk cost is exactly what is keeping the pain in place.

The counter-counter — the reason I still land on "stay" — is that this argument
justifies removing the *divergence*, not the *web platform*, and it does not answer
the a11y point at all. You would trade a debuggable, documented class of engine
differences for an undocumented class of missing accessibility, and PRODUCT.md
commits to the latter in writing. But if the a11y bar were relaxed, or if Slint's
licence question resolved favourably, this argument would win.

## 7. What I could not determine

- **Whether Tauri's Servo/Verso unified-webview work is production-viable.** This
  would materially weaken the one real pro-migration argument. Not verified.
- **Slint licence compatibility as a legal matter.** I established the SPDX conflict
  (GPL-3.0-only vs AGPL-3.0-only) and that the royalty-free tier targets proprietary
  apps. Whether a commercial Slint tier resolves it for an AGPL-3.0-only product is a
  licensing question, not an engineering one. **This is the one open item worth
  escalating** — Slint is otherwise the best a11y match in the field.
- **Actual screen-reader behaviour of any candidate.** I read dependency graphs,
  issue trackers, and docs. I did not run NVDA, VoiceOver, or Orca against any
  toolkit. "Depends on accesskit" is not "works with NVDA"; treat every a11y column
  entry as `untested` at the behavioural level.
- **Current PlateVault a11y conformance.** 541 `aria-*` uses and a J16 journey show
  intent; the journey is `status: draft` with open issues #747 (Inbox has zero
  keyboard shortcuts) and #797 (sidebar lacks focus-visible). So the baseline is not
  perfect either — but it is measurable and remediable, which is the difference.
- **Whether `crates/e2e-tests` journeys could be reproduced in-process.** The
  in-process harnesses exist; whether they can express a 16-file journey suite
  against a real SQLite backend is unproven.
- **gpui's `gpui_platform` split status.** Documented at HEAD, absent from published
  0.2.2. I could not determine when or whether it will be released.
