# Verification — App shell left navigation: destinations, groups, active states, collapse, root health

> Two-stage verification plan. Stage 1 is executed by an agent driving the REAL
> Windows app (real backend, `VITE_USE_MOCKS=false` — mock mode is forbidden)
> over the Tauri MCP bridge. Stage 2 is a human-judgment visual pass in Claude
> Desktop and runs ONLY after Stage 1 fully passes.
> Shared launch / reset / recompile / bridge mechanics live in
> `e2e-agentic-test/AGENT-RUNNER.md` (authored on the sibling lane
> `verify-plans-setup-sources`) — follow it for anything not repeated here.

## Scope and spec references

- Spec 043 (UI redesign PlateVault) §3 "Cross-cutting rules / shared
  primitives" and §5b (per-page consistency): grouped left sidebar is the app's
  primary navigation.
- Spec 046 FR-001/FR-004 (labels come from the Paraglide catalog; English,
  locale pinned) — nav labels are render-time thunks (spec 046 #8).
- Source of truth: `apps/desktop/src/app/Sidebar.tsx` on branch
  `redesign-ui-platevault`.

Ground truth the app MUST match (from `Sidebar.tsx`):

| Group label | Item id | Label | Route |
|---|---|---|---|
| Capture | `inbox` | Inbox | `/inbox` |
| Library | `sessions` | Sessions | `/sessions` |
| Library | `calibration` | Calibration | `/calibration` |
| Library | `targets` | Targets | `/targets` |
| Work | `projects` | Projects | `/projects` |
| Work | `archive` | Archive | `/archive` |
| (pinned bottom, own section) | `settings` | Settings | `/settings` |

Other invariants: active item carries class `alm-sidebar__item--active` AND
`aria-current="page"`; the `<nav>` has class `alm-sidebar` and an aria-label;
brand row shows mark "P" + brand name + version; a collapse chevron button with
an aria-label toggles class `alm-sidebar--collapsed`; footer (only when
expanded) shows a root-health line ("N roots · M online" style) that links to
Settings → Data Sources (`/settings/data-sources`); Inbox / Sessions /
Calibration / Targets / Projects can show count badges
(`alm-sidebar__item-badge`, Inbox variant `--alert`).

## Preconditions (both stages)

1. Windows checkout `C:\dev\astro-plan` is on the branch under test
   (`origin/redesign-ui-platevault` or the PR branch being verified):
   `git fetch origin` then `git reset --hard origin/<branch>` (own command).
   Frontend-only surface — no forced Rust recompile needed.
2. App launched per AGENT-RUNNER.md with the dev overlay config so the bridge
   is up: `cargo tauri dev --config src-tauri\tauri.dev.conf.json`
   (`--features dev-tools` plus `PV_MCP_BRIDGE_ENABLE=1`, without which the
   bridge does not start; bridge WS on `127.0.0.1:9223`, so a NAT-gateway
   connection from WSL also needs `PV_MCP_BRIDGE_BIND=0.0.0.0` at launch).
3. First-run setup completed with at least ONE registered source root (any
   empty folder as Light frames), so the app lands on the shell, not `/setup`.
4. `.env` has `VITE_USE_MOCKS=false`. Verify: `mcp__tauri__ipc_get_backend_state`
   returns real data, or `status_summary`-backed sidebar counts change with DB
   state. Mock mode = automatic FAIL of the whole scenario.

## Stage 1 — Agent validation via Tauri MCP

Connect: `mcp__tauri__driver_session` with `host=<WSL gateway IP>` `port=9223`.

### 1.1 All destinations present, correctly grouped and ordered

1. `webview_dom_snapshot` (or `webview_execute_js`) and extract, in DOM order,
   every `.alm-sidebar__item` inside `nav.alm-sidebar` with its `href`/route,
   label text, and enclosing `.alm-sidebar__group`'s
   `.alm-sidebar__group-label` text.
   - Expected: exactly the 6 grouped items from the ground-truth table, in that
     order, under group labels "Capture", "Library", "Work" — plus a 7th
     "Settings" item rendered inside `.alm-sidebar__settings` (NOT inside any
     workflow group).
   - FAIL if: any destination missing/extra, wrong group, wrong order, or
     Settings rendered inside the grouped nav.
2. Assert the `<nav>` element has a non-empty `aria-label` attribute.
   - FAIL if missing/empty.
3. Assert brand header: `.alm-sidebar__mark` text is `P`,
   `.alm-sidebar__brand-name` is non-empty and not a raw i18n key (no
   `shell_brand_name` literal), `.alm-sidebar__version` non-empty.
4. `webview_screenshot` → checkpoint `nav-expanded.png`.

### 1.2 Active state follows the route

For EACH of the 7 destinations (Inbox, Sessions, Calibration, Targets,
Projects, Archive, Settings):

1. Click its sidebar item via `webview_interact` (selector: the
   `.alm-sidebar__item` whose label matches).
2. Assert via JS:
   a. `window.location.pathname` (or the router state) starts with the item's
      route.
   b. Exactly ONE `.alm-sidebar__item` carries `alm-sidebar__item--active`,
      and it is this item.
   c. The same element has `aria-current="page"`; all others have NO
      `aria-current`.
3. FAIL if: zero or multiple active items, `aria-current` missing on the
   active item, or active visual class on a non-current item.

Screenshot checkpoints: `active-inbox.png`, `active-settings.png` (two are
enough; assert the class/attribute programmatically for all seven).

### 1.3 Collapse / expand

1. Read the collapse button's `aria-label` (must be a translated
   "collapse"-meaning string, not a raw key).
2. Click it. Assert:
   - `nav` now has class `alm-sidebar--collapsed`.
   - `.alm-sidebar__item-label` spans are gone (icon-only items).
   - Each `.alm-sidebar__item` still has a non-empty `aria-label` AND a
     `title` attribute equal to its label (tooltip affordance while collapsed).
   - Footer root-health block is NOT rendered.
3. `webview_screenshot` → `nav-collapsed.png`.
4. Click the (now "expand") chevron again. Assert the collapsed class is gone
   and labels are back.
5. Persistence: collapse the sidebar, then reload the webview
   (`webview_execute_js`: `location.reload()`), wait for render
   (`webview_wait_for` on `.alm-sidebar`). Assert the sidebar is STILL
   collapsed (preference `sidebarCollapsed` persists). Expand it again to
   restore state.
   - FAIL if collapse state resets on reload.

### 1.4 Badges and root-health footer are backend-driven (real IPC)

1. `mcp__tauri__ipc_monitor` (start capture), reload the webview, then
   `ipc_get_captured`: confirm the status-summary/list commands the sidebar
   depends on fire against the real backend and return `Ok` (no
   `command not found`, no mock short-circuit).
2. Read the footer `.alm-sidebar__roots` text. Assert it reports the SAME
   total/online root counts as the backend (cross-check against the
   registered-sources listing IPC response captured above, or by invoking the
   relevant command via
   `window.__TAURI__.core.invoke`).
3. Click the footer root-health line. Assert navigation to
   `/settings/data-sources` (Settings page opens with Data Sources pane
   active). Deep pane behavior itself is covered by the setup/sources lane's
   scenarios — do not duplicate here.
4. If the Inbox badge is visible, assert its count equals the backend inbox
   count; badge styling class must be `alm-sidebar__item-badge--alert`.
   (If the library is empty and no badges render, record "no badges present —
   count 0" as PASS-with-note; do not fabricate data.)

### 1.5 Log check

`mcp__tauri__read_logs`: no new `error`-level entries were produced by any of
the steps above. FAIL on any error entry attributable to navigation.

Stage 1 verdict rubric: PASS only if every numbered assertion above passed
(PASS-with-note allowed only where explicitly stated). Any FAIL blocks Stage 2.

## Stage 2 — Final Claude Desktop pass

Human-judgment checks on the real app window (do not start unless Stage 1 is
green):

1. At the default window size AND at minimum 1100×720: the sidebar is fully
   usable — no clipped labels, no overlapping badge/label, group labels
   legible, Settings and the footer never pushed out of view.
2. Active-item styling is visually unmistakable in BOTH a light theme
   (Warm Slate) and a dark theme (Observatory) — switch via Settings →
   General. Contrast of label text and badge text is comfortable in both.
3. Collapse animation/behavior feels intentional: icons centered, tooltips
   appear on hover while collapsed.
4. Labels read as natural English product vocabulary (Inbox, Sessions,
   Calibration, Targets, Projects, Archive, Settings) — no raw keys, no
   truncation, no Title-Case inconsistencies.
5. Sign-off: record PASS/FAIL per point with a screenshot at 1100×720 in each
   theme.

Final verdict: scenario passes when Stage 1 = PASS and Stage 2 = PASS.
