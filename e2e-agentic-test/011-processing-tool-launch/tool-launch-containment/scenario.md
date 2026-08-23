# Two-stage verification — Tool launch + working-directory containment

> Area: PROJECTS · Spec 011 (processing-tool-launch)
> Shared runner mechanics: see `e2e-agentic-test/AGENT-RUNNER.md`.
> Stage 1 MUST fully pass before Stage 2.

## Change facts (context)

- Spec 011 FRs under test: FR-001 (per-tool executable path in Settings),
  FR-002 (launch from project actions), FR-003 (launch unavailable/blocked
  without a configured executable), FR-004 (every attempt audited), FR-005
  (launch never mutates lifecycle), FR-008 (success returns `launch_id` that
  spec 012 observes), FR-009 (folder argument only when
  `supports_open_folder`, else project root as cwd only), **FR-010 (cwd is
  canonicalized and MUST resolve inside a registered library root, else the
  launch is refused with `cwd.outside_library_root`)**, FR-011 (no pre-spawn
  existence check; OS spawn error → `launch.failed` + `os_error_kind`).
- Commands: `tools_list`, `tools_update`, `tools_validate_path`,
  `tools_discover`, `tools_launch`.
- UI: project detail action bar → `[data-testid="tool-launch-btn"]`, label
  "Open in {tool}" / working label "Launching…"; disabled with a tooltip when
  no executable is configured. Toasts: "Launched {tool}" /
  "Failed to launch {tool}: {error}". One-time hint for tools that take no
  folder argument: "{tool} doesn't accept a folder to open — open your project
  files from inside the app." (localStorage `alm.toolhint.cwdAnchored.<toolId>`).
  Relaunch confirm modal testids: `relaunch-modal`, `relaunch-confirm`,
  `relaunch-cancel`.

## Preconditions — setup / reset + fixture recipe

1. Deploy `origin/redesign-ui-platevault` per AGENT-RUNNER.md.
2. Fixture:
   a. Fresh DB → first-run setup (Lights + Inbox + Projects folders);
      ingest fixture lights; create project `Launch Test` with one source so
      it is launch-eligible.
   b. Configure a harmless real executable for the project's tool in
      Settings → processing tools: `C:\Windows\System32\notepad.exe`
      (exists, spawns instantly, no side effects). Keep a SECOND tool
      unconfigured for the disabled-state test.
3. Window 1100×720; real backend only.

## Stage 1 — Agent validation via Tauri MCP

Connect per AGENT-RUNNER.md; `ipc_monitor` on.

### Test 1.1 — Disabled state without executable (FR-003)
1. Open a project whose tool has NO executable configured (or clear the path
   via Settings first).
2. Expected: `tool-launch-btn` is disabled with an explanatory tooltip
   (`title` attribute present, mentioning configuration — not empty); no
   `tools_launch` can be triggered by clicking.
3. FAIL if: button enabled without a configured path, or click fires IPC.

### Test 1.2 — Successful launch (FR-002/FR-004/FR-005/FR-008)
1. Open `Launch Test` (tool configured to notepad.exe); click
   `tool-launch-btn`.
2. Expected:
   - Button shows "Launching…" then returns; toast "Launched {tool}".
   - Captured `tools_launch` request carries the project id; response
     contains a `launch_id` (FR-008) — record it.
   - A `notepad.exe` process exists
     (`Get-Process notepad`) — kill it afterwards.
   - `projects_get` re-read: lifecycle UNCHANGED (FR-005).
   - Log panel / audit shows a launch record (FR-004) — check via the bottom
     log viewer or `read_logs` for the launch audit event.
   - Screenshot: `s1-launch-success.png`.
3. FAIL if: no `launch_id`; lifecycle changed; no audit trace; toast absent.

### Test 1.3 — Working-directory containment (FR-010) — the core check
1. Verify the honest path first: from Test 1.2's captured request/response
   and backend logs, confirm the launch cwd is the project's folder (inside
   the registered Projects root).
2. Now force a violation via the bridge (the UI never offers an outside cwd,
   so this drives the contract directly). Reuse the EXACT captured request
   shape from Test 1.2, changing only the working-directory/project-path
   input if the request models it; if the request only carries ids (cwd is
   derived server-side), instead re-point the project at an outside path:
   create throwaway dir `C:\Temp\outside-root`, then update the project's
   folder path to it via `projects_update` (bridge), and click launch.
3. Expected: the launch is REFUSED with error code
   `cwd.outside_library_root` in the captured response; the UI surfaces
   "Failed to launch {tool}: {error}" (error text may embed the code); NO
   process is spawned (`Get-Process notepad` empty); no lifecycle change.
4. Restore the project path afterwards (`projects_update` back).
5. FAIL if: launch succeeds with a cwd outside every registered root; error
   code differs; a process spawned anyway.
   NOTE: if `projects_update` refuses path changes in this lifecycle, report
   the exact refusal and mark 1.3 BLOCKED with evidence — do not fake it.

### Test 1.4 — Spawn-failure path (FR-011)
1. Set the tool executable to a nonexistent path
   `C:\definitely\missing\tool.exe` (Settings or `tools_update`); click
   launch.
2. Expected: captured error `launch.failed` with a descriptive
   `os_error_kind`; toast "Failed to launch {tool}: {error}"; button
   re-enabled for retry (not stuck on "Launching…").
3. Restore notepad.exe afterwards.
4. FAIL if: a pre-spawn validation error appears instead of the OS spawn
   error contract, or the button wedges.

### Test 1.5 — Artifact observation wiring smoke (FR-008 → spec 012)
1. With the project open after a successful launch, drop a file into the
   project's output folder:
   `New-Item 'C:\dev\astro-plan\test-data\project-1\Launch Test\output\master_test.xisf' -Force`
   (adjust to the project's real folder from `projects_get`).
2. Expected: the Tool Launches accordion
   (`[data-testid="tool-launches-accordion"]`) gains an artifact row for the
   file within a few seconds (per-project watcher, see the
   012 artifact-attribution scenario for depth).
3. FAIL if: nothing appears even after reopening the project (rescan path).

### Test 1.6 — Logs & layout
1. `read_logs`: no panics/uncaught errors across 1.1–1.5.
2. 1100×720: action bar (with Reveal · Open in {tool} · transitions) pinned;
   only content scrolls.

Stage 1 verdict: PASS = 1.1, 1.2, 1.4, 1.6 green AND 1.3 shows the refusal
contract (or documented BLOCKED with evidence); 1.5 FAIL downgrades to a
warning here only if the 012 scenario is run in the same campaign.

## Stage 2 — Final Claude Desktop pass (human judgment)

1. Launch UX: click "Open in Notepad" (or the configured tool label) — the
   tool window appears in front; the app gives immediate feedback; judge that
   a real user would not double-click (button disabled while working).
2. Relaunch path: launch twice; if the relaunch confirm modal
   (`relaunch-modal`) appears, judge its copy clarity; Cancel must be a true
   no-op.
3. Failure copy: with the broken path from 1.4, judge the toast is
   actionable (names the tool, hints at settings) and not a raw code dump.
4. cwd-anchored hint: for a tool profile without `supports_open_folder`,
   confirm the one-time hint reads naturally and does NOT reappear on the
   second launch.
5. Themes: disabled state and failure toast in `warm-slate` and
   `observatory-dark`.
6. Layout 1100×720: action bar never wraps off-screen with long tool names.
7. Sign-off PASS/FAIL + screenshots.
