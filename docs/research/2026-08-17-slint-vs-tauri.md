# Slint vs Tauri for platevault -- greenfield technical comparison

Date: 2026-08-17. Slint version assessed: **1.17.1** (released 2026-07-07, `gh release list --repo slint-ui/slint`; `docs.rs/crate/slint/latest` = 1.17.1). Tauri surface assessed: **tauri 2.x** with plugin set from `apps/desktop/src-tauri/Cargo.toml` at `origin/main`.

Scope per owner: greenfield choice. Migration cost excluded. Accessibility excluded.

Verified vs claimed: statements marked **[verified]** come from a command result or a file at `origin/main`; **[docs]** come from Slint's own documentation or crates.io/GitHub metadata; **[inference]** is reasoning from those.

## Licence -- settled, not re-litigated

GPLv3 §13 permits combining a covered work with an AGPLv3 work into a single combined work, with the AGPL network clause governing the combination. platevault is AGPL-3.0-only (`apps/desktop/src/ui/index.ts:2` SPDX header) [verified]. Slint's free GPLv3 option is therefore compatible at zero cost. The earlier "licence blocks this" verdict was wrong. Slint also sells a Startup & Individual tier (owner reports ~$10/mo) relevant only if platevault were ever closed-sourced; the pricing page lists that tier's eligibility (≤10 employees, ≤2M EUR turnover, <5 years) but no figure in fetched content [docs].

One licence-adjacent cost does exist: Slint's pricing page lists a **GUI Test Framework** as a paid add-on on lower tiers [docs]. The free introspection route is `i-slint-backend-testing`, triple-licensed `GPL-3.0-only OR ...` [docs], which is AGPL-compatible -- see §9.

## 1. Renderers

Slint ships four renderer paths [docs, backends_and_renderers]:

| Renderer | Requirement | Documented tradeoff |
|---|---|---|
| Software | none; CPU only | "Runs anywhere, highly portable, and lightweight"; **no rotation/scaling, no `drop-shadow-*`, no `border-radius` with `clip: true`, no text stroking**; "Text rendering currently limited to western scripts" |
| FemtoVG | OpenGL required (wgpu variant adds Metal/Vulkan/D3D) | "Text and path rendering quality sometimes sub-optimal" |
| Skia | OpenGL/Metal/Vulkan/D3D | "Heavy disk-footprint compared to other renderers"; documented compile troubles (MSVC 2022 requirement, `BINDGEN_EXTRA_CLANG_ARGS` on ARMv7, Windows paths with spaces) |
| Qt | Qt present at compile time | "only supports software rendering at the moment" |

`SLINT_BACKEND=winit-software` selects software at runtime [docs].

**Fit for this app.** platevault is table-and-form heavy but not static: it uses Leaflet maps (`apps/desktop/src/shared/observing-sites/SiteLocationPicker.tsx`) [verified], visx SVG charts (`@visx/*` in `apps/desktop/package.json:57-60`) [verified], and animated shells. The software renderer's "no drop-shadow, no scaling, western scripts only" limits are load-bearing: pt-BR is a shipping locale (`apps/desktop/project.inlang/settings.json`) [verified] -- Latin script, so western-only is survivable, but any future non-Latin locale is not. Elevation shadows and rotate transforms would have to be redesigned out.

The GPU-variability argument is real but narrower than it sounds. A webview inherits the host WebView2/WKWebView/WebKitGTK compositor and its driver blocklists; Slint software rendering does eliminate that class of variability outright, at the cost of CPU-bound repaint. Skia reintroduces exactly the same GPU-driver surface the webview has, plus its own build fragility.

I could **not** verify binary size, startup time, or battery numbers for any renderer -- no benchmark was run and Slint publishes none in the pages fetched. Treat all size/startup claims as unmeasured.

**Verdict: Slint wins narrowly on the elimination-of-webview-variability axis (software renderer only), and that win is paid for in missing visual primitives. For a desktop app with maps and charts, Skia is the realistic choice, which forfeits the variability win entirely.** Effectively a draw.

## 2. Internationalisation

Repo baseline [verified]: `@inlang/paraglide-js` ^2.21.0, **2,254 keys in each of `apps/desktop/messages/en-GB.json` and `pt-BR.json`** (counted with a JSON key count, `$`-prefixed keys excluded), 188 lines containing `plural`/`match` in en-GB, compile step `paraglide-js compile --strategy custom-almSettings preferredLanguage baseLocale --emit-ts-declarations` (`apps/desktop/package.json:33`), runtime switching in `apps/desktop/src/data/locale.tsx`, and three CI guards: `check-i18n-catalog.mjs`, `check-i18n-locale-drift.mjs` (`apps/desktop/package.json:16`).

Slint [docs, translations guide]: `@tr("...")` where "the first argument must be a plain string literal"; `{}` and `{0}` interpolation; plurals via `@tr("I have {n} item" | "I have {n} items" % count)`; context via `"ctx" =>` mapping to gettext `msgctx`; extraction with `slint-tr-extractor` → `.pot` → `.po` → `msgfmt` `.mo`.

Two delivery modes, and the difference matters:

- **Runtime gettext**: requires the `gettext` crate feature, catalogs at `dir/locale/LC_MESSAGES/domain.mo`, domain **must equal the Cargo package name**. "For gettext-based loading, locale comes from environment variables set by the OS, so no explicit runtime switch API is documented" [docs].
- **Bundled**: `with_bundled_translations()`, and `slint::select_bundled_translation` does switch language at runtime [docs]. But "selecting a translation also selects its decimal separator" at compile time [docs].

So in-app locale switching without restart exists only on the bundled path. Paraglide gives it unconditionally, plus typed message functions (`--emit-ts-declarations`) so a missing or renamed key is a **type error**. Slint's `@tr()` takes a literal string: a typo in the msgid silently falls back to the source text -- the exact failure mode paraglide's generated types remove. Gettext plurals cover only the two-form family well; ICU-style select/match in the current catalogue would need restructuring. Tooling: poedit/Lokalize/Transifex for Slint vs the inlang editor plus two custom drift checks already in CI.

**Verdict: Tauri/paraglide wins clearly.** Typed keys, unconditional runtime switching, richer plural forms, and drift enforcement already automated.

## 3. Widget inventory

Slint Std-Widgets, complete list [docs, std-widgets overview]: Palette, StyleMetrics, Button, CheckBox, ComboBox, ProgressIndicator, RadioGroup, Slider, SpinBox, Spinner, StandardButton, Switch, LineEdit, ListView, ScrollView, StandardListView, StandardTableView, TabWidget, TextEdit, GridBox, GroupBox, HorizontalBox, VerticalBox, AboutSlint, **DatePickerPopup**, **TimePickerPopup**. Language-level: `MenuBar`, `ContextMenu` (added 1.10.0, 2025-02-28), `DragArea`/`DropArea` (1.17.0, 2026-06-24), `SystemTrayIcon` (1.17.0) [verified from `CHANGELOG.md` via GitHub API].

Repo primitives exported from `apps/desktop/src/ui/index.ts` [verified]: Pill, Btn, Section, Box, KV, EmptyState, Skeleton, Table (+`tableIndent`), Banner, Toggle, NumberField, SegControl, RadioGroup, CoverageBar, Lock, DirPicker, WizardShell, ToastContainer, InfoTip, Tooltip, ResizeHandle, `useAdaptiveDock`. Plus app-level components on npm libraries: `cmdk` command palette (`apps/desktop/src/app/CommandPalette.tsx`), `@tanstack/react-virtual` in six places (`LogPanel.tsx`, `useFollowTail.ts`, `TargetSearch.tsx`, `useTargetSearch.ts`, `CalendarScroll.tsx`, `TargetList.tsx`), `react-joyride` tours, `react-hook-form` + `zod` forms, `@base-ui-components/react` for overlays [verified].

Concrete gap table:

| Need | Slint status | Work required |
|---|---|---|
| Virtualised long list | **Has it.** ListView: "elements are only instantiated if they are visible, which guarantees stable performance" with a "practically unlimited number of items" [docs] | none |
| Virtualised table | Partial. StandardTableView reuses ListView optimisation (CHANGELOG 1.2.x "use `ListView` optimization for all styles" #3425) [verified] | see next row |
| **Rich table cells** | **Missing.** StandardTableView rows are `StandardListViewItem` whose only documented field is `text` [docs] -- no per-cell buttons, pills, icons, progress bars | Hand-roll a table on ListView + layouts. This is the single largest build item; platevault's tables carry Pill/CoverageBar/Lock/Btn cells [verified via `ui/index.ts`] |
| Tree / expandable rows | **Missing.** Issue #505 "TreeView Widget" open since 2021-09-15; #4218 "Datastructure that represents a tree" open [verified]. Third-party `slint-tree-view` 0.2.1 (2026-07-26) exists [verified crates.io] | hand-roll or take an unproven 0.2 crate |
| ComboBox | Has it (text items) | none for plain; hand-roll for rich/searchable |
| Date picker | Has `DatePickerPopup`/`TimePickerPopup` | none |
| Modal / overlay | `PopupWindow`, `ContextMenu` | thin |
| Toasts | **Missing** | hand-roll (overlay + timer queue); small |
| Resizable split panes | **Missing** -- no Splitter widget in Std-Widgets | hand-roll on TouchArea drag; small-medium |
| Command palette | **Missing**, and no fuzzy matcher | hand-roll: overlay + `nucleo`/`fuzzy-matcher` + keybinding. Medium |
| Charts | **Missing** | `Path` elements hand-rolled, or `ruviz-slint` 0.11.0 (2026-08-16, brand-new) [verified crates.io] |
| Interactive map | **Missing** | `slint-mapping` 0.1.0 (2026-05-18) [verified crates.io] -- 0.1, unproven. Leaflet has no equivalent |
| Guided tour (joyride) | **Missing** | hand-roll |

**Verdict: Tauri wins decisively.** Slint covers the primitive layer and virtualised lists well; it does not cover rich table cells, trees, maps, charts, or a palette, and those are not decoration in this app.

## 4. Rust integration and the `.slint` DSL

Mechanics [docs]: `.slint` files compile ahead of time via `slint-build` in `build.rs`, and `slint::include_modules!()` pulls the generated Rust into the crate; the `slint!` inline macro is the alternative. Generated per component: a struct with typed getters/setters per exported property and `on_<callback>` registration; models cross as `ModelRc`/`VecModel`. `docs.rs/slint/1.17.1` documents a `generated_code` pseudo-module and notes its "described structure is not really contained in the compiled crate" [docs].

Costs, evidenced:
- It is a real language with its own reactivity model -- Slint publishes a "reactivity-vs-react" page [verified in sitemap], which is itself an admission of the learning delta.
- The `slint!` macro re-parses builtin widgets on every expansion, ~31% of expansion time (issue #12096, opened 2026-06-14, open) [verified].
- `slint-updater` 1.16.1 exists as a tool to migrate `.slint` files across Slint versions [verified crates.io] -- the DSL has breaking syntax churn.

Buys: LSP with editor plugins for VS Code, Kate, Qt Creator, Helix, Vim, Sublime, JetBrains, Zed [docs]; a formatter; and **live preview inside the real app** -- "runs inside your actual application with your real data and callbacks", "the application keeps running during reloads -- properties, callbacks, and models are preserved", errors non-fatal [docs]. Limit: renaming a property or callback used from Rust "may kill the app -- Recompile and restart" [docs].

Versus JSX for these screens: JSX composes arbitrary expressions and hooks; `.slint` composes declarative elements with a constrained expression language. For form-heavy wizard screens (`WizardShell`, `SetupWizard`) the declarative form is arguably clearer. For anything computing layout from data (`useAdaptiveDock`, virtualised calendars) the logic moves to Rust and crosses a typed boundary each way.

**Verdict: draw on ergonomics.** Slint's live preview is genuinely better than React Fast Refresh for pure-visual iteration because it preserves live state; the DSL's version churn and constrained expressions offset it.

## 5. Compile-time safety -- the crux

This is the section the decision should hinge on, so it is answered bug by bug, including the misses.

Repo context that shapes every answer [verified]: the frontend is mid-migration to **vanilla-extract** -- 56 `.css.ts` files vs 24 remaining plain `.css` files under `apps/desktop/src`; commit `f736644bc` "migrate UI primitives and shared components to type-safe vanilla-extract styles (#1572)"; `eb8d2c5c5` "validate every CSS design-token reference (#1635)"; `8c0a8a6ac` "enforce lifecycle-string and CSS-selector ratchets in CI (#1604)"; guard scripts `scripts/check-pv-selector-ratchet.sh` (sealed at zero new `.pv-*` selectors in e2e), `scripts/check-tokens.sh`, `scripts/check-lifecycle-strings.sh`.

### (a) Conditional `data-testid` broke 39 e2e specs while 2,376 unit tests passed

**Slint would NOT have caught this.** `data-testid` is a test-affordance concept, not a type. The Slint analogue is `ElementHandle::find_by_accessible_label` / `find_by_element_id` [docs, `i-slint-backend-testing` 1.17.1], and `find_by_element_id` requires `SLINT_EMIT_DEBUG_INFO=1` at build time. A conditionally-set accessible label is just as invisible to the compiler as a conditional attribute. What changes is the *shape* of the failure: element ids in Slint derive from declared element names in the `.slint` source rather than from a hand-written string, so the "someone made the hook conditional" mistake is less natural to write. But the compiler does not reject it. **Verdict: NO -- mitigated by convention, not caught.**

### (b) A removed CSS rule let sticky headers overlay a button

**Slint would likely have caught this, structurally.** There is no `position: sticky` and no z-index stacking cascade in Slint; overlap is expressed by explicit layout containers (`VerticalBox`, `GridBox`) or explicit absolute coordinates. Deleting a property in a layout-managed tree changes geometry deterministically rather than un-suppressing an overlap. Two caveats: Slint does not "catch" it with an error message -- it just makes the class of bug much harder to construct; and if you *do* hand-place elements with absolute `x`/`y`, the overlap is fully expressible and uncaught. **Verdict: MOSTLY YES -- eliminated by construction in layout-managed code, not by a diagnostic.**

### (c) Nine dead BEM selectors silently dropping styles

**Slint would have caught this -- and so does vanilla-extract, which this repo is already adopting.** In `.slint`, styling is a property on an element; there is no selector to go stale, so the failure mode does not exist. Note the honest comparison: with 56 `.css.ts` files already migrated [verified] and `check-tokens.sh` plus #1635 in CI, the *current* stack has largely closed this too. A dead `style` export in vanilla-extract is caught by `knip` (`apps/desktop/package.json:35`) [verified]. **Verdict: YES -- but the delta against the repo's own trajectory is shrinking, not static.**

### (d) A stale `className` rendering unstyled text

**Slint would have caught this.** Same reason as (c): no string indirection between an element and its appearance. Under vanilla-extract, a renamed export is also a TS error; the bug was possible only in the not-yet-migrated plain-CSS remainder (24 files) [verified]. **Verdict: YES -- and the current stack catches it too, in migrated code only.**

### (e) A token-emission bug bypassing the runtime type scale

**Slint would NOT have caught this.** This is a generator producing wrong-but-well-typed output. Slint's equivalent design-token layer is a global singleton of properties, and a build step that emits a wrong value into a `global Tokens` block compiles cleanly and renders wrongly, identically. Only a value-level assertion catches it -- which is what `scripts/check-tokens.sh` and #1635 are [verified]. **Verdict: NO -- same class of bug, same required guard.**

### Score and the honest read

Three of five would be caught or structurally eliminated (b, c, d); two would not (a, e). Every one of the three is a **CSS-indirection** bug, and all three are the bugs the repo's vanilla-extract migration and token-validation CI already target. The two Slint would miss -- test-affordance drift and a bad generated value -- are the two that cost the most in this session.

**Verdict on §5: Slint's compile-time safety is real but concentrated in exactly the area the repo is already fixing by other means.** Adopting a whole toolkit to close CSS indirection, when `.css.ts` + a token validator closes it at 56/80 files today, is not a proportionate trade. This weakens the strongest pro-Slint argument rather than strengthening it.

## 6. Where Tauri + React genuinely wins

- **Ecosystem, quantified.** In-use libraries with no Slint equivalent: `@tanstack/react-query` 5.101.2 (server-state cache, retry, invalidation), `@tanstack/react-router` 1.170.17, `@tanstack/react-virtual` 3.14.5, `cmdk` 1.1.1, `leaflet` 1.9.4, `@visx/*` 4.x, `react-hook-form` 7.81.0 + `zod` 4.4.3 + `@hookform/resolvers`, `react-joyride` 3.2.0, `astronomy-engine` 2.1.19, `date-fns` 4.4.0, `tinykeys` 4.0.0, `lucide-react` 1.24.0 [verified, `apps/desktop/package.json:44-74`]. Replacing TanStack Query alone means hand-writing request dedup, cache invalidation, and retry over the Tauri/IPC boundary.
- **Design-token pipeline.** `style-dictionary` 5.5.0 → `build-tokens.mjs` → `gen-ve-themes.mjs` → typed vanilla-extract theme contract, with `--check` modes wired into `pnpm lint` [verified `package.json:16,28-32`]. Slint has a Figma variable exporter [docs] but nothing comparable to a token-source-of-truth build with CI drift checks.
- **Restyling by a designer.** CSS is a skill market; `.slint` is not.
- **Iteration loop.** Vite 7.2 HMR plus Vitest 4.1 in watch mode against jsdom. Slint's live preview preserves state better, but Rust recompiles for any logic change dominate the loop.
- **Devtools.** Webview inspector gives computed styles, layout, network, profiler. Slint offers `debug()` to stderr, `SLINT_SLOW_ANIMATIONS`, `SLINT_SCALE_FACTOR` (FemtoVG/Skia only), `SLINT_DEBUG_PERFORMANCE`, and RenderDoc for GPU capture [docs]. That is a materially thinner inspection surface.

## 7--8 + expansion: OS-integration capability parity

Real plugin set at `origin/main` [verified, `apps/desktop/src-tauri/Cargo.toml:52-66` and plugin registration at `src/lib.rs:193-239`]: `tauri-plugin-dialog`, `tauri-plugin-opener`, `tauri-plugin-mcp-bridge`, `tauri-plugin-single-instance`, `tauri-plugin-window-state`, `tauri-plugin-log`, `tauri-plugin-updater`, `tauri-plugin-process`, plus `tauri-plugin-webdriver` 0.2 behind the `e2e` feature and `tauri-specta`.

**Correction to the brief:** the brief's list omitted `tauri-plugin-mcp-bridge` and the `e2e`-gated `tauri-plugin-webdriver`. Also `tauri-plugin-notification` 2.3.3 is declared in the **workspace** `Cargo.toml:95` but is **not** registered in `lib.rs` [verified] -- consistent with `specs/SPEC_STATUS.md:93` recording US8 as not implemented. And `trash` 5.2 is already a plain crate at `Cargo.toml:112`, used by `crates/fs/executor` [verified] -- not a plugin at all, so it carries over to Slint unchanged.

| Feature | Tauri today (evidence) | Slint equivalent | Gap |
|---|---|---|---|
| Signed auto-update | `tauri-plugin-updater`, minisign pubkey + GitHub `latest.json` endpoint in `tauri.conf.json:60-65`; staged flow in `src/data/updateSubscription.ts` (check → download+verify → deferred install), distinct `check-failed`/`download-failed`/`restart-failed` phases | `self_update` 0.44.0 (2026-07-16). Signature verification only via the non-default `signatures` feature using **zipsign**, and "Artifacts are assumed to have been signed using zipsign"; no checksum verification documented; **no relaunch API** [docs docs.rs]. Alternatives: Sparkle/WinSparkle via bindings (per-OS, two codebases), `cargo-dist` 0.32.0 installers | **HAND-ROLL** |
| Update check / manifest / channels | Plugin defines the `latest.json` format and does the version compare; `PV_E2E_VERSION_OVERRIDE` test seam gated by `semver` (`Cargo.toml:44-47`) | Your own manifest format, fetch (`reqwest`, already in tree), semver compare, and "update available" UI. `self_update`'s GitHub backend does release listing; channels are yours | **HAND-ROLL** |
| Folder / multi-file picker | `tauri-plugin-dialog` via `commands/native.rs:30,62` (`native_directory_pick`, `native_file_pick`) | `rfd` 0.17.2 (2026-01-12, 25.5M downloads) -- async API, Win/mac/Linux | **CRATE-EXISTS** |
| Drop folders onto window | Tauri core emits `tauri://drag-drop` with paths (recorded in `docs/research/tauri-plugin-api-audit-2026-07-05.md:186`) | `DropArea` (1.17.0) accepts drops "from another application on platforms that support it"; **the docs never name the platforms, never mention files, and never mention file paths** [docs, droparea + drag-and-drop guide]. `data-transfer` "abstracts over the file-type transfer mechanisms supported by each platform"; only plain-text MIME helpers are documented. Open crash: #12437 "Crash when dragging file onto winit window" (2026-07-10, labelled `upstream`) [verified] | **UNAVAILABLE as verified** -- could not confirm path delivery on any platform; a live spike is required before trusting it |
| Reveal in Finder/Explorer | `tauri-plugin-opener` `reveal_item_in_dir` + Linux `xdg-open` fallback + audit, `commands/native.rs:100-185` | `open` 5.4.1 / `opener` 0.8.5 open a path but, per the repo's own audit (`docs/research/tauri-plugin-api-audit-2026-07-05.md:208`), "neither reliably *selects/highlights* the item" | **HAND-ROLL** (per-OS: `open -R` on macOS, `explorer /select,` on Windows, DBus FileManager1 on Linux) |
| Trash / recycle bin | `trash` 5.2 crate, `crates/fs/executor` | identical crate | **NONE** |
| Filesystem watching | `notify` >=8,<9 + `notify-debouncer-full` in `crates/fs/inventory/Cargo.toml:14-15` | identical crates | **NONE** |
| System tray icon | not used today (no tray reference in `src-tauri` [verified]) | `SystemTrayIcon` element, 1.17.0: `NSStatusItem`, `Shell_NotifyIcon`, StatusNotifierItem/AppIndicator. Limits: exactly one `Menu` child not inside `if`/`for`; `shortcut` bindings ignored; globals not shared with the Window; `title` no-op on Windows; on macOS `clicked` doesn't fire with a populated menu; "plain X11 system trays are not supported" [docs] | **NONE** (Slint is arguably ahead here) |
| Native menu bar | `bootstrap/menu.rs:15-59` builds App/Edit/Window submenus from `PredefinedMenuItem::{about,quit,undo,redo,cut,copy,paste,select_all,minimize,close_window}` + `Settings…` with `CmdOrCtrl+,` | Slint `MenuBar` (1.10.0) with `shortcut` on `MenuItem` (1.16.x) [verified CHANGELOG]; winit path builds via **muda** (issue #9792 references "muda menubar build") [verified]. No documented predefined-item set equivalent to Tauri's -- standard Edit-menu roles are yours to declare, or drop to `muda` 0.19.3 directly | **CRATE-EXISTS / partial HAND-ROLL** |
| Window state persistence | `tauri-plugin-window-state`; app adds `enforce_min_window_size` and `recenter_if_offscreen` with 11 unit tests (`bootstrap/window.rs:65,82`) | nothing. Serialise position/size/maximised yourself; Slint's window API exposes them. The offscreen/multi-monitor logic already lives in the app's own tested code and carries over | **HAND-ROLL** (small) |
| Single instance | `tauri-plugin-single-instance`, registered `lib.rs`; **focuses the existing window** on second launch | `single-instance` 0.3.3 (last published **2021-12-16**, 862k downloads) or `fslock`; a raw lock blocks but does not focus. Focus-existing-window is per-OS IPC you write | **HAND-ROLL** |
| Splash → main handshake | two windows declared in `tauri.conf.json:13-31` (`main` `visible:false`, `splash` transparent/undecorated/centred), driven from `src/splash/main.ts` | Slint supports multiple windows and undecorated/transparent windows; issue #11001 "Slint aggressively overrides window properties" (open, 2026-03-12) and #2521 custom-titlebar proposal (open since 2023) are the risk markers [verified] | **CRATE-EXISTS with risk** |
| Taskbar progress / overlay badge, jump lists, Dock badge | not used today | nothing in Slint. Raw `windows` crate `ITaskbarList3` / `objc2` `NSDockTile` | **HAND-ROLL** |
| OS notifications | `tauri-plugin-notification` 2.3.3 declared (`Cargo.toml:95`) but **not registered**; spec 051 US8 open per `specs/SPEC_STATUS.md:93` | `notify-rust` 4.18.0 (2026-06-16). The repo's own audit already rates it "viable" for backend-triggered notifications (`tauri-plugin-api-audit-2026-07-05.md:210`) | **CRATE-EXISTS** -- fair comparison, unbuilt on both sides |
| Diagnostics log file | `tauri-plugin-log` + `tracing-appender` for the rotating file (`Cargo.toml:71`), `lib.rs:217` uses `skip_logger()` | `tracing` + `tracing-appender` unchanged -- the plugin is only the JS-side bridge | **NONE** |
| Crash reporting | none today [verified: no sentry/crash crate in `Cargo.toml`] | `sentry` or `minidumper`/`crash-handler`, identical on both | **NONE** |
| Packaging, notarisation, Authenticode | `cargo tauri build` + `@tauri-apps/cli` 2.11.4; signing pipeline merged (`SPEC_STATUS.md:93` cites `60732f2f`/#469) | `cargo-packager` 0.11.8 (2025-11-27, 165k downloads) is the closest and is by ex-Tauri authors; `cargo-dist` 0.32.0 (2026-05-22) does archives/installers/CI but is CLI-shaped; otherwise hand-rolled CI per platform | **HAND-ROLL** -- this is where the hidden cost lives |
| E2E driving | `tauri-plugin-webdriver` 0.2 behind `e2e` feature + Playwright 1.61 over 35 spec files [verified] | `i-slint-backend-testing` 1.17.1: `find_by_accessible_label`, `find_by_element_id`, `single_click`/`double_click`, headless via `SLINT_BACKEND=headless`. Explicitly "should **not be used directly** by applications", "does not follow the semver convention", breaking changes "in any patch release", pin `=x.y.z`. **No documented keyboard API.** MCP server for agent-driven inspection (`SLINT_MCP_PORT`) | **CRATE-EXISTS with sharp edges** |
| IPC contract typing | `tauri-specta` + `specta-typescript` generate the TS surface from Rust (`Cargo.toml:48-49,62`), contract parity tested | no boundary to cross -- the UI is Rust. This is the cleanest structural win for Slint | **NONE (Slint better)** |

**Verdict 7--8: Tauri wins, and signed auto-update is the decisive item.** The current flow is check → download → minisign-verify → defer install → explicit relaunch, with three distinct failure phases and a test seam [verified in `updateSubscription.ts`]. The best Slint-side answer, `self_update` with the non-default `signatures` feature, verifies zipsign-signed archives and has no relaunch. Reproducing today's behaviour means: own the manifest format, own the signing (zipsign or minisign directly), own the atomic swap on three platforms, and own relaunch. Folder drag-and-drop is second: unverifiable from docs and with an open upstream crash.

## Added dimension: contract boundary (the one real structural win)

Going Slint deletes an entire layer: no `tauri-specta` codegen, no `contracts_core` → TypeScript projection, no JSON serialisation per call, no dual test stacks (Vitest 4.1 + Playwright 1.61 alongside `cargo test`). A UI callback becomes a Rust function call. Against that, the repo's Tier-1/Tier-2 durability model (constitution V) and all business logic already live in Rust crates -- the boundary is doing real architectural work today, not just ceremony.

## Recommendation

**Greenfield, for THIS app: Tauri + React.**

The reasoning is concentrated, not diffuse. platevault is not a control panel; it is a table-and-data application with a Leaflet map, four visx chart families, a fuzzy command palette, guided tours, six virtualisation sites, and rich interactive table cells. Slint's Std-Widgets stop at text-only table cells, have no tree after five years of an open issue (#505), and have no chart or map story beyond 0.1-version third-party crates. Add signed auto-update, which Slint cannot match at any level, and unverifiable folder drop into the window on an app whose core loop is "point me at a folder".

The compile-time-safety case -- the one genuinely strong argument for Slint -- turns out to target CSS indirection specifically, and the repo is 56/80 files into a vanilla-extract migration plus a token validator in CI that closes the same class. Two of the five real bugs would have shipped under Slint identically.

## Strongest argument against my own recommendation

**The bug ledger is the evidence, and it favours Slint.** In one session this stack produced five defects, three of them pure presentation-layer indirection, and one of them (the conditional `data-testid`) broke 39 of 35+ e2e spec files while 2,376 unit tests stayed green. That is not bad luck; it is what a stack with three uncoordinated naming systems -- CSS classes, test ids, and token strings -- produces structurally. Slint deletes two of the three. My counter-argument leans on the vanilla-extract migration *finishing* and the token validator *holding* -- a trajectory, not a state. If that migration stalls at 56/80, or if the widget gaps could be closed once by a competent Rust developer (a table on ListView, a palette on `nucleo`, charts on `Path`) and then never regress, then a Slint build trades a permanent class of runtime styling bugs for a bounded one-time widget build. That is a defensible trade, and someone who values eliminated bug classes over ecosystem breadth should make it.

## What I could not determine

1. **Binary size, startup time, and battery draw** for any Slint renderer -- no benchmark run, none published in the pages fetched. Every size/startup statement above is unmeasured.
2. **Whether Slint's `DropArea` delivers OS file paths from a file-manager drop, on any platform.** The docs say drops from other applications work "on platforms that support it" and never name the platforms or mention files. Requires a spike on macOS, Windows, and Linux.
3. **Whether `StandardTableView` supports user column-drag resizing.** `width`/`min_width`/`horizontal_stretch` exist; the docs never say the user can drag.
4. **The Startup & Individual tier price.** The pricing page fetch returned no monetary figures; the owner's ~$10 is unconfirmed against the vendor page.
5. **Whether the paid GUI Test Framework is required for practical e2e**, or whether `i-slint-backend-testing` suffices given its no-keyboard-API and patch-level-breakage caveats.
6. **Slint's software-renderer text quality for pt-BR diacritics** -- "limited to western scripts" is documented; rendering fidelity was not evaluated.
