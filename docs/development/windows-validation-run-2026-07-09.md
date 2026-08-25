# PlateVault — Windows real-app validation run (2026-07-09)

Interactive validation of the real Tauri desktop app on Windows, driven from WSL
via the tauri MCP bridge (`localhost:9223`, mirrored networking). Ten documented
journeys (`docs/development/windows-journeys/journey-01..10-*.md`) executed step by
step. **Interactive protocol:** every test stops for human review before the next.

**Run focus (per human directive, restart 2026-07-09):**
1. Restart the whole run from a clean first-run state.
2. Validate every step with an explicit human checkpoint.
3. Rigorously test **input validation** — actively feed wrong/edge entries and
   confirm the app catches them, not just the happy path.
4. Log **every** issue (human-noticed or agent-noticed) in the persistent
   backlog below.

## Environment

- **Commit under test:** `8097d9c6` (Windows checkout `C:\dev\astro-plan`).
  origin/main is `c6b82435`, 5 commits ahead, but all 5 are `docs:` / `chore:
  release` / `ci:` commits — **zero product-code diff** — so the running binary
  is behaviourally identical to current main.
- Real backend: `VITE_USE_MOCKS=false`, `VITE_E2E=1` (exposes `data-testid`
  stand-ins for the native folder pickers).
- DB (first-run source of truth): `sqlite://C:\dev\astro-plan\wizard-test.db`.
- Bridge: WebSocket `0.0.0.0:9223`, connect `driver_session host=localhost
  port=9223`. Repeating this run needs `--features dev-tools` at launch, and
  `PV_MCP_BRIDGE_BIND=0.0.0.0` to reproduce that bind; the bridge binds
  `127.0.0.1` by default and is absent without the feature.
- Run branch / worktree: `ws3-mcp-validation-run` @
  `/home/sjors/tmp/worktrees/astro-plan/campaign-ws0` (tracks origin/main).

## Persistent backlog

Every issue lives here for the life of the run. Status: `OPEN` / `FILED #NN` /
`FIXED` / `WONTFIX` / `INFO`. Severity: `bug` / `validation-gap` / `doc-drift` /
`enhancement` / `info`.

| ID | Sev | Where | Summary | Status |
|----|-----|-------|---------|--------|
| B1 | doc-drift | journey-01 doc | Doc says "Step 1 of 5"; wizard is now 6 steps (added Observing Site, 044-US3). | OPEN — fix doc in run branch |
| B2 | enhancement | Wizard · Observing Site | No map-based location picker; lat/long typed by hand. spec:044. | FILED #491 |
| B3 | validation-gap | Wizard · Step 1 Source Folders | **RESOLVED into specifics this run.** Client buffer accepts *every* invalid path (nonexistent, illegal `<>\|`, relative, a file) with no error; only exact duplicate is (silently) deduped. Backend `register_source` (used by both single + batch/Confirm) correctly rejects: nonexistent→`path.not_exists`, file→`path.not_directory`, relative/illegal→`path.not_exists`, duplicate→`path.already_registered`. **Residual real gaps split out to B9/B10/B11.** | RESOLVED → B9/B10/B11 |
| B4 | info | Project wizard · Calibration (Journey 5) | Prior run: tool auto-detection is REAL on this build. Open issue #327 claims the Project-wizard Calibration step renders hardcoded mock masters — verify explicitly in Journey 5. | INFO — verify J5 |
| B5 | info | Wizard nav | Prior run: folder-card buffer survives Back/Forward navigation. Re-confirm. | INFO |
| B6 | doc-drift | Run mechanics / reset recipe | Documented fresh-install reset ("wipe `wizard-test.db` only") is **incomplete**: the wizard folder buffer (`alm-setup-wizard-state`), theme (`alm.theme`), favourites, cleanup decisions, and path hints all persist in `localStorage`. A true fresh-install reset needs DB wipe **AND** `localStorage.clear()`. Confirmed this run: DB wipe alone rehydrated 5 stale folder cards + `warm-clay` theme. Clearing LS with an already-wiped DB caused **no** redirect loop. | OPEN — note in journey docs |
| B7 | enhancement | Wizard · Step 1 category order | Two **required** categories are interleaved with optional ones: order is Light frames (required) → Calibration (optional) → Projects (**required**) → Inbox (optional). A required input (Projects) sits below an optional one — weak information hierarchy. Human-noticed. Recommend required-first grouping + Required/Optional divider. | FILED #496 |
| B8 | enhancement / a11y | App-wide · option controls | **Zero** contextual help affordances (verified: no `title`, `aria-describedby`, `role=tooltip`, or `?` icons on Step 1). Human request: add a keyboard/SR-accessible (?) help tooltip to every relevant option (folder categories, organized/unorganized, and app-wide — scan depth, remap, cleanup actions, calibration matching, etc.). | FILED #497 |
| B9 | **bug** | Backend · `register_source` (roots_register + roots_register_batch) | **Overlapping roots are accepted.** With `…\ALM test\Lights` registered, registering nested child `…\Lights\1` AND parent `…\ALM test` both succeed — via the single command **and** the batch path the wizard Confirm uses (all 3 items `status: success`). No `path.overlaps` check exists. Violates required rules: a root must not be *within* another root (#3) nor a *parent* of another (#4). Overlap → double-scan / ambiguous ownership / duplicate ingest. Evidence: commit 8097d9c6. | FILED #501 (bug) |
| B10 | validation-gap / UX | Wizard · Step 1 (frontend) | Buffer accepts any string as a folder card with **no add-time validation** — invalid paths only fail later at Confirm/register. Exact-duplicate dedup is **silent** (no "already added" message). Recommend validating the 4 rules at add-time with inline SR-accessible feedback. | FILED #502 |
| B11 | **bug** | Backend · `register_source` | Duplicate-path error `path.already_registered` has **`severity: "warning"`**; it must be **`blocking`** (registration cannot proceed on a duplicate). Human-directed. | FILED #501 (folded) |
| B12 | **bug** | Wizard · Step 3 Configuration | **Theme selector broken**: dropdown has only **one option ("Light")** and its value doesn't match the live theme (`observatory-dark`), so it can't toggle the theme. Theme mechanism itself works (global `<html data-theme>`). Human-noticed + confirmed. | FILED #504 (bug) |
| B13 | ux / bug | Wizard · Step 3 Configuration | **Display density has no live effect in the wizard.** Value persists to `alm-preferences` but the wizard renders outside the Shell (`Shell.tsx:96` applies `density-${d}` to `.alm-frame` on main pages only), so no preview. Inconsistent with theme (which is global). Confirm density works on main pages post-setup. Human-noticed. | FILED #505 |
| B14 | evaluation / tech-debt | Source protection (spec 016/018) | **NOT deprecated** — wired + consumed by cleanup/permanent-delete safety (`plans.rs` `PlanBlockedByProtection`), per-source override exists (`SourceProtectionOverride.tsx`) but only in Settings, wizard has global default only. Human evaluation: (1) surface per-source protection on Step 1 cards; (2) likely **drop the configurable default** — hardcode a sensible pre-fill like org/depth do — to cut the 3-surface complexity. | FILED #506 (+2 design comments) |
| B15 | observation | Wizard · Step 5 Confirm | Confirm summary lists per-folder **scan depth** but not the **organized/unorganized** state. Minor — consider showing org too. | OPEN (minor) |
| B16 | observation | Wizard · Step 4 Observing Site | **Name accepts empty** (Continue stays enabled) — site name not required. Lat/long ARE properly validated (see positive result). Decide if name should be required. | OPEN (minor) |
| B17 | **bug** + product decision | Scanner · `scan_depth` (spec 003) | **`single` depth is a no-op** — `scan_dir` always recurses; `ScanOptions` has no depth field; `ScanDepth::Single` is never read (only written). Recommendation (human-directed): **drop single/recursive entirely, always recursive** — no realistic advantage, conflicts with no-overlap (#501), astro libs are hierarchical, and single-level silently loses files. | FILED #509 (bug + drop rec) |
| B18 | ux / polish | Wizard · Step 2 Processing Tools | Layout inconsistent ("vibe-coded"): redundant green pills (**"Detected"** header pill + separate **"OK"** path pill = same meaning), **"Redetect" text button** should be a retry (↻) icon, and small-caps `EXECUTABLE` label + pill styling don't match the design system. Human-noticed (screenshot). | FILED #510 |
| B19 | **bug** / validation-gap | Wizard · Step 2 "Select binary…" | Manual binary override does **zero validation**: native dialog is unfiltered (`*.*`); a picked **`.zip`** was accepted as the PixInsight binary with a green **"OK"** pill and Continue enabled. Decision: drop `*.*` (executables only; Linux needs post-pick exec-bit check), require executable, **don't run** the binary for identity (GUI-launch risk), version-surfacing best-effort no-exec only (Win PE / macOS plist; not Linux). Auto-detect + Redetect work fine. Human-noticed. | FILED #511 |
| B20 | enhancement / a11y | Wizard · step-nav tabs | Step tabs (`1. Source Folders … 6. Scan`) are inert `<div>`s (not buttons, no role, not focusable) — nav is Back/Continue only. Request: make clickable (jump back freely; forward respects gates) + real interactive/keyboard a11y. Human-requested. | FILED #512 |
| B21 | **bug** | Wizard · Step 6 Scan preview | Folder-expansion table: **detected types don't reconcile with file count** (unclassified/master files silently omitted — e.g. inbox 6 files shows "2 flat, 2 light"), **empty root-row name**, masters not surfaced. Root cause of "missing darks": master-dark fixtures have stripped headers (no IMAGETYP) + non-master name → correctly unclassified but hidden. Human-noticed. | FILED #513 |
| B22 | test-coverage | spec-040 master detection | Detection is header-first (IMAGETYP; path/name fallback), well unit-tested for main cases but gaps: bias raw, bias-master-STACKCNT, darkflat full detect, path-based type inference, **header-vs-filename conflict**, master lights, and no realistic-FITS integration test. Add comprehensive permutation "library test" + fixtures. Human-directed. | FILED #514 |
| B15 | minor | Wizard · Step 5 Confirm | (filed) Confirm shows depth not org. | FILED #515 |
| B16 | minor | Wizard · Step 4 Observing Site | (filed) Empty site name allowed. | FILED #516 |

> B1–B5 were observed by the **prior** run session on this same build. Under the
> restart directive they are carried forward as claims to **re-verify from
> scratch**, not as settled facts.

## Journeys

_(populated as the run proceeds; each test: `Test N — name / PASS|FAIL|BLOCKED /
observed evidence / deviations / issues filed`)_

### Journey 1 — First-run setup → data sources

Environment: commit `8097d9c6`, fresh DB **and** cleared `localStorage`.

**Fresh-install prep (this run):** killed `desktop_shell`/`node`/`cargo`,
`Remove-Item wizard-test.db*`, relaunched (`Finished in 1.07s` — no recompile,
source unchanged), reconnected bridge. First read showed 5 **stale** folder cards
+ `warm-clay` theme rehydrated from `localStorage`; cleared all 15 LS keys +
reloaded → clean state confirmed. See **B6**.

**Test 1 — Fresh install lands on the setup wizard — PASS.**
- Route `#/setup`; step indicator "STEP 1 OF 6"; heading "Where does your data
  live?"; 0 leftover folder cards; wizard buffer re-initialised empty
  (`sources: []`). No blank window, no redirect loop.
- Deviation from doc: doc says "Step 1 of 5"; app is **6 steps** (B1). Expected —
  the 6th step (Observing Site, 044-US3) shipped after the doc was written.

**Test 2 — Add a Light frames folder — PASS.**
- Added `D:\astrophotography\ALM test\Lights` via the `light_frames` E2E stand-in.
  Card appeared with the path + "Already organized / Needs organizing" toggle
  (default organized); input cleared after add; no toast.
- Proved buffer-only: `roots_list` returned `[]` (0 registered) while the card
  was in the buffer — nothing registered before Confirm, as required.

**Input-validation matrix (Step 1) — mixed; real gaps found.**

_Layer 1 — client buffer (add via wizard UI):_

| Input | Result |
|-------|--------|
| nonexistent `Q:\does\not\exist` | ❌ accepted, no error |
| illegal chars `D:\bad<>\|chars` | ❌ accepted, no error |
| relative `foo\bar` | ❌ accepted, no error |
| file (not dir) `…\Lights\M 51_LUM…0000.fits` | ❌ accepted, no error |
| exact duplicate of Lights | ✅ deduped (silent, no message) |

Client validates **only** exact duplicates → **B10**.

_Layer 2 — backend `register_source` (single `roots_register` + batch
`roots_register_batch`; both share the same validation core):_

| Test | Rule | Outcome | Error code |
|------|------|---------|-----------|
| real Lights (control) | — | ACCEPTED | — |
| duplicate (same path) | not same path | rejected | `path.already_registered` **(severity: warning — should be blocking, B11)** |
| nonexistent `Q:\` | exists | rejected | `path.not_exists` (blocking) |
| file `.fits` | is a directory | rejected | `path.not_directory` (blocking) |
| relative `foo\bar` | exists | rejected | `path.not_exists` (blocking) |
| illegal `<>\|` | valid/exists | rejected | `path.not_exists` (os err 123, blocking) |
| **nested child `…\Lights\1`** | **not within another root** | **ACCEPTED** ❌ | — (no overlap check) → **B9** |
| **parent `…\ALM test`** | **not a parent of another root** | **ACCEPTED** ❌ | — (no overlap check) → **B9** |

Batch path (wizard Confirm) registered all 3 overlapping roots `status: success`
— **no intra-batch overlap detection** either. All probe roots cleaned up
(`roots_list` back to 0); wizard buffer reset to empty.

**Test 3 (in progress) — full org×depth option matrix across all categories.**
Per human directive, testing every `(category, organization, scanDepth)` combo
through the real wizard UI so the settings persist and feed downstream journeys.

Fixture tree built at `D:\astrophotography\ALM test\OptMatrix\` (14 dirs, real
FITS copies, original filenames preserved for detector tokens; sibling dirs, no
overlap):

| Category | Combos | Per-dir content (top / recursive count) |
|----------|--------|------------------------------------------|
| light_frames | organized×{rec,single}, unorganized×{rec,single} | 1 M51 light / 2 |
| calibration | organized×{rec,single}, unorganized×{rec,single} | master dark + raw flat = 2 / 4 (both types) |
| project | organized×{rec,single}, unorganized×{rec,single} | 1 M51 light / 2 |
| inbox | unorganized×{rec,single} (org forced) | light + master dark + raw flat = 3 / 6 (mixed) |

Wizard buffer verified: all 14 cards' org/depth selects match their combo name
(allMatch: true). Inbox org correctly forced `unorganized` (no org select
rendered). Expected scan: `single` depth finds top-level only, `recursive` finds
all — verifying next.

**Wizard traversal + step validation (steps 2–5).**
Registered all 14 via the real wizard (Confirm→Start scan). Steps: 1 Folders →
2 Tools → 3 Config → 4 Observing Site → 5 Confirm → 6 Scan.

- **Step 5 Confirm — PASS + assertion held.** `roots_list` = **0** at Confirm
  (no registration/scan before leaving Confirm ✓). Summary groups all 14 folders
  by category with per-folder depth labels. Shows depth but not org state (B15).
- **Step 4 Observing Site — validation PASS (positive).** lat `999`/`abc` →
  blocked, "Latitude must be a number between -90 and 90."; lon `999`/`abc` →
  blocked, "Longitude must be a number between -180 and 180."; valid → Continue
  re-enables. This is the correct inline+accessible validation the folder-path
  step (B10) lacks. Empty **Name** allowed (B16).
- **Step 3 Configuration — examined (had been skipped).** 4 controls: SIMBAD
  toggle ✓ works; Display density ✗ no wizard effect (**B13** #505); Default
  source protection (**B14** #506 — evaluate/simplify); Theme selector ✗ broken,
  one option (**B12** #504). Density/protection option vocab correct.
- **Step 2 Processing Tools — detection PASS + UI issues.** PixInsight
  (`C:\Program Files\PixInsight\bin\PixInsight.exe`) and Siril
  (`C:\Program Files\Siril\bin\siril.exe`) both **Detected · OK**, both enabled
  (confirms B4: detection is real). UI polish issues → **B18** #510 (redundant
  Detected/OK pills, Redetect should be a retry icon, inconsistent typography).
  **Enable/disable ✓** (toggling off hides the exec path row + sets
  enabled=false). **Redetect ✓** (re-detected the real binary, overriding a bad
  override). **Binary-picker validation gap → B19 #511**: human drove the native
  picker and a **`.zip`** was accepted as the PixInsight binary with "OK" +
  Continue enabled (no filter, no executable check). Step 2 fully validated.

**Scan results (Step 6) — depth semantics OPEN.** All 14 registered + scanned to
"Done". Per-folder counts: light rec/single both **2 files · 2 folders**; cal
both **4 · 2**; proj both **2 · 2**; inbox both **6 · 2**. Every `single`-depth
root shows identical counts to its recursive twin AND "2 folders" (i.e. it
descended into `sub/`). **RESOLVED via code (no 3-level fixture needed):
`scan_depth` `single` is a no-op** — `scan_dir` recurses unconditionally,
`ScanOptions` has no depth field, `ScanDepth::Single` is never read. So all scans
are recursive; the depth option does nothing. **B17 → #509** (recommend dropping
the option entirely). `roots_list.fileCount` reads 0 for all (scan counts live
elsewhere; minor).

**Org (organized/unorganized) — VERIFIED CONSUMED (contrast depth).** Unlike
`scan_depth`, `organization_state` is load-bearing: `crates/app/inbox/src/confirm.rs`
branches on it at ingest — **Organized → catalogue-in-place** (record where the
file is, no move); **Unorganized → move** to the pattern-resolved destination
(confirm.rs:221-298; tests `organized_source_emits_catalogue_actions` /
`unorganized_source_emits_move_actions`). Read per-source. So the org half of the
matrix is meaningful downstream — to verify behaviorally in the Inbox journey
(organized roots catalogue-in-place, unorganized roots move).

**Wizard steps 1–5 all validated.** Step 6 Finish not yet clicked. Pending:
Finish + Test 4 persistence + density on main pages, then Data Sources + Journeys
2–10.
