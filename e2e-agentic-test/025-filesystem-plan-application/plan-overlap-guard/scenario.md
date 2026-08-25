# 025 plans — cross-plan overlap guard (`plan.conflict.overlap`)

> Two-stage verification plan. Stage 1: agent via Tauri MCP bridge; Stage 2:
> Claude Desktop human pass, only after Stage 1 passes.
> Runner mechanics: `../../AGENT-RUNNER.md`. Fixtures:
> `../../041-inbox-plan-surface/FIXTURES.md`.

## Coverage

| Requirement | Assertion in this scenario |
|---|---|
| 025 FR-017 / R-Concur-1 (PR #408) | Two plans whose (source ∪ destination ∪ archive) path sets overlap at subtree-prefix granularity cannot apply concurrently; the second is rejected with `plan.conflict.overlap` |
| PR #408 | Rejected plan is left untouched (stays approved/open; no state change; no leaked registry entry — it applies fine afterwards) |
| PR #408 | Disjoint plans MAY apply concurrently |
| Spec 046 gate | The error surfaces through the localized error-message map (`err_plan_conflict_overlap` catalog message), not as a raw code |

**Convoy precondition: requires PR #408 merged** into the branch under test
(`feat: prevent two plans from applying to the same files at the same
time`, branch `impl-025-overlap-guard`). Before running, verify on Windows:
`git log --oneline -20 | Select-String overlap` or check
`crates/fs/planner/src/path_set.rs` exists at the deployed commit. If not
merged: report BLOCKED, do not improvise.

## Preconditions

1. Deploy the branch containing PR #408 (normally
   `redesign-ui-platevault` after the shepherd merges it). **Rust changed**
   → force the recompile (touch the changed `.rs` files after
   `git reset --hard`; see AGENT-RUNNER.md recompile trap). A stale binary
   makes this scenario meaningless (the guard silently absent).
2. Clean DB + fixtures; generate **RECIPE-OVERLAP** (folders `overlap-a`
   120 lights, `overlap-b` 2 lights colliding with A's destination subtree,
   `overlap-c` 2 disjoint lights) and **RECIPE-DEST**
   (`test-data\library-lights`, single light root).
3. Launch with bridge + `VITE_E2E=1`; window 1100×720; real backend.
4. Wizard: inbox root = `test-data\inbox-drop`; light root =
   `test-data\library-lights` (organized). Finish → `/inbox` → Rescan →
   classify all three items (select each once; the 120-file folder takes
   longer — wait for the detail to resolve).
5. Confirm all three items (each produces one plan; single root →
   auto-selected). Verify `inbox_plan_list_open` shows 3 plans.
6. Approve each of the three plans via `plans_approve` (`{ id: PLAN_ID }`).
   `inbox_plan_apply` refuses a plan that carries no approval with
   `plan.approval_required` (astro-plan-tykek), so this stands in for the
   Apply gesture the UI performs.

## Stage 1 — Agent validation via Tauri MCP

1. **Overlap is rejected while A applies.** Via `webview_execute_js`, in
   ONE evaluation start A's apply un-awaited and immediately await B's
   (fill in the three real plan/item ids from `inbox_plan_list_open`):

   ```js
   (async () => {
     const inv = window.__TAURI__.core.invoke;
     const pA = inv('inbox_plan_apply', { req: { inboxItemId: A_ID } });
     let bResult;
     try { bResult = await inv('inbox_plan_apply', { req: { inboxItemId: B_ID } }); }
     catch (e) { bResult = { rejected: e }; }
     const aResult = await pA.catch(e => ({ rejected: e }));
     return JSON.stringify({ aResult, bResult });
   })()
   ```

   (If the `inbox_plan_apply` arg shape rejects, read the exact request
   type from the generated bindings and paste the corrected call into the
   report.)
   - Expected: `aResult` = success (A applies normally); `bResult.rejected`
     is a ContractError with code **`plan.conflict.overlap`** (paste it
     verbatim).
   - Timing note: A has 120 file actions precisely so its apply window is
     wide; if B still lands after A finished (no rejection, both succeed),
     the run is INCONCLUSIVE, not PASS — retry once with a fresh DB and
     240 files in A before reporting.
   - FAIL if: B applies concurrently with A over the same destination
     subtree (guard absent), or B fails with a DIFFERENT error code.
2. **Rejected plan is untouched and applies afterwards (PR #408).** After
   step 1 completes, re-read `inbox_plan_list_open`.
   - Expected: B's plan still open/approved (not stale, not errored, not
     half-applied); now `invoke('inbox_plan_apply', {…B_ID…})` alone
     SUCCEEDS (re-`plans_approve` B first if the rejection cleared its
     approval); B's 2 files land under
     `test-data\library-lights\NGC 7000\Ha\...\light\` (WSL `find`), and no
     duplicate/partial artifacts exist.
   - FAIL if: B is stuck (registry leak — the guard didn't clean up), or
     its second apply is rejected with `plan.conflict.overlap` again
     despite A being finished.
3. **Disjoint plans are allowed (R-Concur-1 positive case).** Reset to a
   fresh state (new DB, regenerate RECIPE-OVERLAP with A at 120 files),
   classify + confirm A and C. Run the step-1 snippet with C in place of B.
   - Expected: BOTH succeed — C (`M 16/OIII/...` subtree) is disjoint from
     A (`NGC 7000/Ha/...`), so concurrent application is permitted; all
     files land correctly on disk.
   - FAIL if: C is rejected with `plan.conflict.overlap` (over-broad
     guard).
4. **Localized surfacing (spec 046 gate).** Trigger the overlap through the
   UI: repeat the A+B setup, click `[data-testid="plan-apply-all"]` in the
   review overlay ORDERED such that A and B are both pending (apply-all
   applies sequentially — if it serializes and never overlaps, instead
   click `plan-apply-one-<A>` then immediately `plan-apply-one-<B>` while
   A's progress bar is running).
   - Expected: if the overlap fires in the UI path, the user-visible error
     is the CATALOG message for `err_plan_conflict_overlap` (an English
     sentence explaining the conflict), NOT the raw code string
     `plan.conflict.overlap`. If the UI path cannot be made to overlap
     (sequential apply-all), record "UI overlap not reachable —
     sequentialized" with evidence and treat the localized-message check as
     satisfied by inspecting `apps/desktop/src/lib/error-messages.ts`
     mapping at the deployed commit.
   - FAIL if: the raw error code is shown to the user.
5. **Audit + logs.** `audit_list` and `read_logs`.
   - Expected: A's and B's applies each audited per action; the overlap
     rejection logged as a structured refusal, not a panic/crash; no
     ERROR-level unexpected entries.
   - FAIL if: the rejection produced a crash or left no trace.

Screenshot checkpoints: `S1-ovl-01` (both plans in overlay pre-race),
`S1-ovl-02` (post-race state: A applied, B still open), `S1-ovl-03`
(UI error surfacing, if reached).

### Stage 1 verdict

PASS = steps 1–5 pass (step 1 must produce a real `plan.conflict.overlap`
rejection, not an inconclusive race). FAIL otherwise, with the JSON results
of the race snippet, `find` output, and screenshots. INCONCLUSIVE races
after one 240-file retry → report BLOCKED-FLAKY with timings.

## Stage 2 — Final Claude Desktop pass

1. Reproduce the overlap once via the UI (two apply-one clicks in quick
   succession, A large). Judge: if the rejection surfaces, is the message
   understandable ("another plan is touching these files") and does it tell
   the user what to do (wait/retry)? Is B visibly still intact afterwards?
2. Judge non-blocking behavior: while A's 120-file apply runs, the app
   stays responsive (progress bar `plan-progress-<A>` animates; the list
   and navigation still work).
3. Theme pass: the error toast/banner in one light + one dark theme.
4. i18n: the overlap message is a real catalog sentence.
5. Sign-off: PASS/FAIL per point + screenshots. Overall PASS requires
   Stage 1 PASS and no unresolved defect.
