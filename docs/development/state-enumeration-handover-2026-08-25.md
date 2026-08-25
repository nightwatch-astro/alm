# PlateVault -- remaining-work enumeration for a system move

**Point-in-time enumeration, true as of 2026-08-25. Not a live status board.**
Every count below -- 762 open beads, 224 in one audit family, 69 spec entries,
the per-priority totals in section 1.3 -- was measured on 2026-08-25 and starts
drifting immediately. Re-run the command a section quotes before acting on its
number. Section 6 records what this enumeration did not establish; read it
before treating any section as complete.

Handover entry point: bead `astro-plan-y3bjh`, which carries the PR and
work-location index. This file was committed under bead `astro-plan-kye7i`.

Written 2026-08-25 for a reader with no history on this project who cannot ask
questions. Every count states what was counted and when, with the command.
Where something could not be established it says so.

Repository read: `<repo>`, branch
`fix/apm-dead-deps`, HEAD `38d07af07` (`git rev-parse --abbrev-ref HEAD`;
`git log -1`, exit 0). The session that opened this work reported branch
`chore/journey-validation-formula`, so the primary checkout's branch moved
during the session -- do not assume either name.

---

## Machine-specific values

Placeholders stand in for values that belong to one machine, using the same key
as `docs/development/journey-prep-2026-08/README.md`:

- `<repo>` for the repository checkout
- `<scratch>` for the scratch directory holding artifacts that are not in git
- `<agent-memory>` for the agent memory directory
- `<journey-host>` for the address of the Windows host
- `<user>` for the Windows account name

Substitute the real value before running a command that quotes a placeholder.
The host address and the `<scratch>` paths that section 0 lists are recorded on
`astro-plan-y3bjh` and its children.

The `<scratch>/journey-prep/` drive plans and triage report that section 0 lists
are preserved in the repo by PR #1760, under
`docs/development/journey-prep-2026-08/`.

---

## 0. REQUIRED MIGRATION STEPS -- do these before the old machine is gone

These artifacts are NOT in git and are NOT in any bead. On a system move they
are lost unless copied.

| Path | Size | What it is |
|---|---|---|
| `<scratch>/journey-prep/J01/drive-plan.md` | 789 lines | Complete offline drive plan, J01 |
| `<scratch>/journey-prep/J02/drive-plan.md` | 680 lines | Complete offline drive plan, J02 |
| `<scratch>/journey-prep/J03/drive-plan.md` | 352 lines | Complete offline drive plan, J03 |
| `<scratch>/journey-prep/J04/drive-plan.md` | 750 lines | Complete offline drive plan, J04 |
| `<scratch>/journey-prep/J06/drive-plan.md` | 644 lines | Complete offline drive plan, J06 |
| `<scratch>/journey-prep/triage/astro-plan-6w2v2-report.md` | -- | Triage of the 17 static prep findings |
| `<scratch>/pv-bringup/` | 264 MB | Working raw-WebSocket bridge client (`bridge.py`), launch/build scripts, overlays, captured screenshots, logs |
| `<scratch>/handover/_ev/` | -- | The evidence files behind this document |

Commands: `wc -l <scratch>/journey-prep/J*/drive-plan.md` (exit 0, 3215
total), `du -sh <scratch>/journey-prep <scratch>/pv-bringup`
(exit 0 -- 236K and 264M), `find <scratch>/journey-prep -maxdepth 2`
(exit 0).

Each drive plan is substantial, source-derived work: every selector was read
out of React source rather than taken from documentation, because the
documentation was found wrong (see trap T12). J01's plan contains 109 lines
matching `^| *E[0-9]` (`grep -c`, exit 0) -- the brief that commissioned this
document said "84 Expects", which does not reproduce; treat 109 table rows as
the measured figure and the count itself as unimportant next to the fact that
the plan exists.

`<scratch>/pv-bringup` is 264 MB and is mostly build output and logs; if
space matters, `bridge.py`, `build.sh`, `launch.sh`, `devlaunch.sh`,
`*overlay*.json` and `shot.json` are the irreplaceable parts. **Untested**
whether the build output is regenerable without the Windows host.

A previous session lost all of `/tmp/u-*.md` and `/tmp/bead-*.txt` the same
way. That loss already happened and is acknowledged on `astro-plan-ip9p7`.

Also machine-local and not in the repo:
`<agent-memory>/`
holds 40 memory notes (`ls -1`, exit 0) that encode most of Part 4. Copy the
directory.

---

## 1. BEAD STATE

All counts from one snapshot: `bd list --json` and `bd list --status open
--json`, both exit 0, taken 2026-08-25.

- 756 non-closed beads total: **702 open**, **52 in_progress**, **2 blocked**.
- By priority across all non-closed: P0 8, P1 143 (122 open + 21 in_progress),
  P2 364 open, P3 156 open, P4 56 open.
- By type: 565 task, 139 bug, 21 chore, 16 epic, 14 feature, 1 event.
- 244 of the 756 belong to one family, `astro-plan-3v3r*` -- the 2026-08
  code-audit finding tree. 23 belong to `astro-plan-kyo7*` (the 2026-07
  quality audit). So **roughly a third of the backlog is two audits' output**,
  not independent product work. Read that before despairing at 702.

### 1.1 The release hold

`astro-plan-78kk` (P0, open): **release PR #1393 (`chore: release main`,
v0.7.0) is HELD by explicit owner instruction.** Verbatim intent recorded on
the bead: do not release until all beads are actually closed and all specs
fully implemented -- "and even then you need my approval". Green CI authorizes
nothing. `gh pr view 1393` (exit 0) shows state OPEN, not draft, `mergeable:
MERGEABLE`, `mergeStateStatus: BLOCKED`.

The hold is a **scope decision and not a defect**. The bead records #1393 as fully
green: Release Gate SUCCESS, three platforms green, cargo-deny / Playwright /
llvm-cov / Real-UI smoke green, release build compiles with `dev-tools` OFF and
the dev surface proven absent. v0.7.0 carries 3 breaking changes: the
`ALM_`->`PV` env/storage-key rename (#1531) and the frozen SQLite schema
baseline (#1453).

What the release actually waits on, from `bd dep tree astro-plan-78kk`
(exit 0) -- 4 blocking edges, 2 already closed:

| Blocker | Status | Note |
|---|---|---|
| `astro-plan-44tb` | open P2 | stale `tasks.md` paper: 052 claims shipped work; 030/005/022/014 superseded |
| `astro-plan-ltoy` | open P1 | validate user journeys once the backlog closes |
| `astro-plan-w59q` | closed | backlog triage sweep |
| `astro-plan-lll4` | closed | spec-010 claimed 31 tasks for zero code |

So mechanically only two edges remain. **But the stated exit condition is "all
beads closed", which the dependency graph does not model.** Do not read a
two-edge tree as "nearly releasable".

Two other P0 beads are navigational, not work: `astro-plan-vano` ("START HERE"
session anchor, 72 comments, newest-first, carries the standing holds) and
`astro-plan-ip9p7` (the 2026-08-25 handover epic this document supplements).
`astro-plan-merge-slot` is a mutex token, not work.

Standing holds recorded on `astro-plan-vano`:

- #1393 is held.
- Merge beads are authorized. The owner said "use merge beads" on 2026-08-18.
- Journey validation starts only after the backlog closes.
- The Windows host is <journey-host> over ssh.
- Passing journeys is **not** a release trigger.
- Never `--no-verify`.
- Push github.com over dgit.
- Check out with `wt new` or `wt switch`, never `git worktree`.

### 1.2 In-flight work with a PR

`gh pr list --state open --limit 100` (exit 0) -- exactly 4 open PRs:

| PR | Draft | Branch | Subject |
|---|---|---|---|
| 1759 | yes | `fix/pidalive-relaunch-guard` | re-launch warning on Windows and macOS |
| 1758 | yes | `fix/casefold-root-overlap` | one folder registering twice as two roots |
| 1757 | yes | `feat/parins-parallel-journey-instances` | run N journey-driving instances on one host |
| 1393 | no | `release-please--branches--main` | the held release |

`gh pr list --state merged --limit 20` (exit 0): #1756 (macOS desktop lane
declared on all 18 journeys) merged 2026-08-25T05:37Z and #1755 (plan-gated
Prepare edge) merged 2026-08-25T02:40Z. #1737-#1754 merged 2026-08-24.

Each open draft has a merge bead: `astro-plan-zhds2` (->#1757),
`astro-plan-9labh` (->#1758), `astro-plan-cf5ec` (confirm #1759). 11 open beads
have titles beginning `merge:`/`pr:merge` (python count over the snapshot,
exit 0) -- more merge beads than open PRs, so some name branches with no PR yet;
`astro-plan-55087` is one (skymath, external repo, owner-gated).

**#1757 matters beyond its own scope**: the port scheme
`9223+(N-1)*100` exists ONLY in #1757 and is unmerged. See the port conflict in
§3.2.

### 1.3 P0/P1 by theme

P0 (8): `bd list` snapshot, exit 0. `lilr5` (this document, in_progress),
`ip9p7`, `vano`, `78kk`, `merge-slot` covered above. The three working P0s:

- `astro-plan-95zbx` (in_progress) -- make journey driving run N concurrent
  instances and **correct the false `exclusive: true` rationale** in
  `docs/journeys/README.md:44-45`. The bead lists six measurements refuting the
  one-host premise, chief among them that
  `tauri-plugin-mcp-bridge-0.11.2/src/discovery.rs:16-20` scans 100 ports
  upward from the base, so 9223 is a default base and not a pin. Beyond the
  title: this false belief is named as the thing that cost the project most of
  its journey throughput.
- `astro-plan-ldj0v` (in_progress) -- verify/purge real-frame copies on the
  journey host. Beyond the title: it resolved a **conflict between two
  validators' reports** (one said all 11 copies deleted, one said 451 MB of
  real frames remain). Its own description still asserts "the host is a SERIAL
  resource: two app instances cannot coexist" -- that assertion is now known
  FALSE (see `astro-plan-mg6h8`). The bead text is stale; the finding is not.
- `astro-plan-dfv3s` (in_progress) -- read-only enumeration of the Windows host
  for the owner, who could not reconcile what they saw on their desktop with
  what agents reported (they saw strings like "tag T3 seen T2" and no splash
  screen). Beyond the title: **it is explicitly forbidden from mutating
  anything**, and it is the bead that records concurrent validators driving.

P1 open (122) and P1 in_progress (21). Themes, with exact ids:

**Journey validation (live front)** -- `xjow5`(J01, ip), `7dmmk`(J03, ip),
`yepf9`(J02), `14fjz`(J06) on Windows; `eoklv`/`6u42d`/`g1fuf`/`zzehh` the
macOS lane for J01/J02/J03/J06; `6w2v2`(ip, triage 17 static findings);
`ba3yl`(ip, J04 prep -- but see §3.1, the plan is finished);
`a9rjq`(add per-journey exclusivity field to FORMAT.md); `ltoy`(the release
blocker); `2szlo`(exploratory testing beyond the journeys); `iizrh`(host not
provisioned). Beyond the titles: the Windows and macOS "drive and validate"
beads are the SAME journeys twice, and a third and fourth family exists -- see
§1.5.

**Host safety / real data** -- `9turs` and `fg3se`. Both are described in §1.4
and Part 4; both are irreversible-consequence beads.

**Bridge/driver defects** -- `ts2r6` (get_window_info reads the wrong arg -- trap
T1), `e5ka0` (verify an unreviewed changelog sentence about
`PV_MCP_BRIDGE_BIND`), `qvmqq` (macOS WebKit localStorage shared across
concurrent instances, defeating isolation -- this is the genuine bug whose
mirror-image false signal is trap T3).

**Spec/doc integrity** -- `2mfx6` (make `specs/` checkable; see Part 2),
`tf9fn` (spec 043 names a dead `--alm-*` token namespace, 4 of 6 themes, wrong
theme storage key), `yhx5v` + `u1aax` (the windows-journeys directory
manufactures false FAILs), `de13a` (APM agent docs teach a dropped PR
contract), `jklcq`/`ko0tv`/`1cdr8`/`bi0w9` (P3 journey-body corrections).

**Product defects with user-visible loss** -- `b2f92` (settings.update reports
success for a value its validator rejected; UI silently reverts),
`i5qxc`(ip, cleanup reclaimable total counts protected bytes no apply can
reclaim), `akon` (project auto-block/auto-unarchive never fire), `4puw`
(a reopened plan has no path back to `ready_for_review`), `6yep` (sessions
groups/proposals UI invokes 15 Tauri commands that do not exist), `6iqa3`
(update-view subsystem not wired to UI, so self-update is unreachable),
`qgyu`/`f1sr`/`vj6x`/`ic9h*`/`ps48`/`b4ng` (session identity vs calibration
heterogeneity, spec 062), `zjf88`(ip, root overlap case-folds only on Windows).

**Tier-1 durability and data-integrity findings from the 3v3r audit** -- these
are the ones a new owner should read first because they touch records the
filesystem cannot re-derive: `3v3r.3.33` (the FR-100 Tier-1 install-intent and
item-journal chain is fully built with **zero production writers**, while
production READS the journal to decide resume-skip), `3v3r.4.30` (first-run
completion relabels `created_via` on EVERY registered source, destroying Tier-1
provenance), `3v3r.6.22` (the documented `framing.project_mismatch` guard on
the Tier-1 attribution path **does not exist**), `3v3r.4.20` (both archive
commands mint an audit id, return it, and never persist it -- permanent deletion
of user files records only a prunable Tier-2 row), `3v3r.8.22`
(`plans.apply.direct` and 3 inbox apply commands mint their own approval token,
defeating the Principle II approval gate), `3v3r.5.22` (an archive item routed
to OS trash bypasses the executor confirm gate), `3v3r.13.22` (SessionKey
delimiter injection collides Tier-1 session attribution), `3v3r.13.23`
(artifact reconcile keys on basename alone), `3v3r.13.24` (ContentHash has no
digest validation in release -- `serde(transparent)` bypasses `from_hex`, which
only `debug_assert`s).

**Identity/injectivity cluster** -- `3v3r.20.21` (`target_id` not injective:
a colon collides two targets), `3v3r.12.25`/`3v3r.12.20` (one normalized alias
owned by two targets / two cameras), `3v3r.6.17` (`file_signature` hashes the
filename via `to_string_lossy`), `3v3r.6.27` (invalid-UTF-8 filename becomes
literal `unknown.fits` and two collide), `5cva1` (standardise target identity
on one normal form -- greenfield, no migration), `3v3r.22` (pattern bead: 11
occurrences over 6 nodes).

**Silent-wrong-value cluster** -- `3v3r.11.13` (`parse_date_obs` fabricates
"tonight" for a garbled DATE-OBS with no confidence marker), `3v3r.6.16`
(`format_num` renders NaN into the session group key, collapsing unrelated
frames into one displayed session), `3v3r.6.15` (non-finite pointing staged
from headers makes every tolerance comparison false), `3v3r.11.37`
(`parse_level` fails open to Unprotected), `3v3r.11.10` (6 of 7 persisted
calibration tolerances never reach the matcher), `3v3r.12.16` (tolerance write
never read back).

**SQL correctness** -- `3v3r.3.13` (`list_plans` interpolates webview filter
strings into executed SQL), `3v3r.12.31`/`3v3r.12.32` (LIKE without ESCAPE; and
the wrong session set for *ordinary* frame ids, no metacharacter needed).

**Concurrency / partial failure** -- `3v3r.21.14` (two concurrent applications
of one plan are not idempotent), `3v3r.21.15` (`reconcile::classify` heals
three distinct in-flight mutations to `succeeded`, including a destination
holding 0 of 5760 bytes and a Relocate whose source delete never ran),
`3v3r.21.17`/`3v3r.3.14` (`item_retry_applying` counter corruption),
`3v3r.3.19` (a prepared view with zero items reaches `current`),
`3v3r.3.18` (eight path aliases let a second project claim an owned
directory), `3v3r.27` (pattern, 4x/3 nodes), `p3oqz` (epic).

**Frontend boundary** -- `3v3r.18.17` (PROVEN: `ipc.ts:96` unwrap throws falsy
`undefined` on six envelope shapes and its error reporter self-throws, on the
boundary shared by all 224 IPC command loci), `3v3r.14.9` (same unwrap is
non-total), `2add` (`packages/contracts/src/generated` has no drift check),
`9w4e2` (epic).

**Build / CI / supply chain** -- `3v3r.19.10` (release profile omits
`overflow-checks`, so `u8::MAX+1` wraps to 0 in every shipped binary),
`3v3r.17.10` (CLA workflow runs an unpinned action from an ARCHIVED repo while
holding a GitHub App private key on `pull_request_target`), `o1om`
(`.gitignore:358` hides tracked `.github/workflows/ci.yml` from every scanner),
`cfbe` (three `tests/contract/*.rs` never compile -- no `[[test]]` entry),
`9f78` (release gate proves dev-tools absence for the Rust binary only, the
frontend bundle is unchecked), `vi96h` (a green commit does not prove
pre-commit ran -- trap T7), `uvgo`, `y0id9` (epic).

**Agent tooling / process (not product)** -- `tpndp`, `ch3vh`, `ar2gp`,
`g3mrw`, `5d3pv`, `qigv2`, `indxl`, `78v0`, `f2vqn`, `qp4dn`, `pci11`,
`86rk8`, `t9lnl`, `f6xgi`, `dpcd`, `70fo`, `36cr`, `e0kw9`, `ptea`, `o3wd`,
`1bmx`, `1bmx.3`, `de13a`, `p7xpf`. A new owner on a new system can defer this
entire theme; none of it is product behaviour. `p7xpf` is the exception worth a
decision -- PR #1706 drops the blocks-edge and PR-trailer gates and needs an
explicit call before landing.

**Meta-audit beads worth reading before doing any work** -- `68c7` (PATTERN:
domain logic written and tested with the invocation site never built), `lljr`
(detector: tables with no production writer -- its counts are self-declared
unreliable, method sound, one confirmed hit), `3v3r.24` (pattern: a self-check
reports success without doing the work, 7x over 6 nodes), `nmo5t` (audit: find
specs/journeys/ADRs/TODOs whose work was never filed as a bead). These four
describe *shapes* of defect that recur here.

### 1.4 Beads that are questions for the owner, not work

- **`astro-plan-9turs` (P1) -- the clearest.** 971 MB of the owner's REAL
  astrophotography frames sit in `C:\Temp\pv-*` on the journey host
  <journey-host>, outside every named throwaway root. 22 FITS, 971,334,720 B,
  all mtimed 2026-07-14, identified by `NAXIS1`/`NAXIS2`/`INSTRUME` rather than
  filename: 17 at 6248x4176 `ZWO ASI2600MM Pro` (52,191,360 B each) and 5 at
  3856x2180 Dwarf 3 (16,816,320 B each). Locations:
  `C:\Temp\pv-inbox-mount` (2), `C:\Temp\pv-inbox-wcs` (2),
  `C:\Temp\pv-journeys` (18). Most are provable copies -- originals found at
  capture-era mtimes under `D:\Astrophotography` for the five
  `M 51_LUM_2025-05-03_*` lights, both `FLAT_LUM_2026-04-19_*` flats, and 2 of
  3 Dwarf 3 darks. **One file has no original anywhere**:
  `C:\Temp\pv-journeys\darks\dark_exp_15.000000_gain_40_bin_1_43C_stack_11.fits`,
  16,816,320 B, 3856x2180, mtime 2026-07-14T07:40:43, while its `_stack_9` and
  `_stack_10` siblings both resolve. **Nothing may touch it.** Combined with
  trap T5 (ssh trash is permanent), a careless cleanup here is unrecoverable
  loss of the owner's data. The owner must decide, not an agent.
  I did NOT look for other files of this shape: doing so requires the Windows
  host, which this unit is forbidden to touch. **That census is unfinished and
  is the open question.**
- **`astro-plan-vi9sp` (P2)** -- its own description says "WHAT TO DECIDE -- the
  census is the work": is the `appDataDir()` false-contamination signal a
  PRODUCT defect or a DOCUMENTATION gap? That is a judgement call, not work.
- **`astro-plan-p7xpf` (P1)** -- "decide before landing".
- **`astro-plan-53b8` (P1)** -- "decide: does Tier-1 durability escalation
  extend beyond the filesystem-mutation intent path?" This is a constitution
  question (Principle V), not an implementation task.
- **`astro-plan-aa2zq` (P1)** -- escalation: `exposure_tolerance_s` has no
  user-settable source and no matching dimension to feed. Needs a product call.
- **`astro-plan-e0kw9` (P1)** -- an ASK bead already shaped as a question.
- **`astro-plan-vkix4` and `astro-plan-a23p5` (P1)** -- both are "establish the
  intended behaviour from git history before treating this as a bug". If git
  history does not answer, they become owner questions.
- **`astro-plan-sz6yt` (P2, ip)** -- 19 of 72 open GitHub issues have no open
  bead; adjudicated as 11 close-eligible, 3 bead-dup. Closing GitHub issues is
  owner authority.

### 1.5 Stale and duplicate beads -- merge candidates, nothing closed here

| Keep | Merge in | Why |
|---|---|---|
| `astro-plan-b2f92` (P1) | `astro-plan-3v3r.8.25` (P2) | Same defect: `settings.update` returns Ok for a rejected value. b2f92 carries full evidence with file:line (`commands/settings.rs:220-222`, `:203-206`, `validation.rs:18-26`, `settingsWrite.ts:27-37`); 3v3r.8.25 has an **empty description** and 1 comment. Keep b2f92. |
| `astro-plan-u1aax` (P2) | `astro-plan-yhx5v` (P1) | Same `docs/development/windows-journeys/` staleness. u1aax holds the stronger evidence: a per-file `git log --follow` provenance test showing 10 of 11 files unchanged since 2026-07-17 (journey-02 since 2026-07-24) and the 2026-08-22 touch being `13353a066` (MCP gating, #1701), not a refresh; plus a testid census correcting the earlier over-report to ONE file citing testids. **But u1aax is P2 and yhx5v is P1** -- merge into u1aax and raise it to P1, or merge into yhx5v carrying u1aax's evidence across. Do not lose the provenance command. Related already-filed: `astro-plan-npts7`. |

Journey-validation beads exist in **four overlapping families** -- this is the
largest duplication in the tracker and a new owner will otherwise plan the same
work four times (`python3` regex census over the snapshot, exit 0):

1. P1 "drive and validate JXX **against the running app**" -- J01 `xjow5`,
   J02 `yepf9`, J03 `7dmmk`, J06 `14fjz`.
2. P1 "drive and validate JXX **on the macOS lane**" -- J01 `eoklv`,
   J02 `6u42d`, J03 `g1fuf`, J06 `zzehh`.
3. P2 "drive and validate JXX on the **windows/macos lane**, with evidence and
   a run record" -- two beads each for J04, J05, J07-J18 (e.g. J07 `1upbo`
   windows / `ddkck` macos).
4. P2 legacy "Validate JXX ... **against the real product** (draft -> active)"
   with timestamp-shaped ids `astro-plan-1784553672431-20-02e611ad` and
   siblings, visible for J11-J17.

Family 4 is superseded by family 3 and should be adjudicated. Family 1 vs 2 is
a real distinction (two OS lanes) but the P1/P2 split across families 1/2/3 is
inconsistent: J01-J03 and J06 are P1 on both lanes while J04-J18 are P2 on
both. Plus 18 "JXX journey epic" beads and 14 "offline prep for JXX" beads.

`astro-plan-mg6h8` (blocked P1) is the umbrella "run the journeys against a
real app". It is **not stale but it is self-contradicting by design**: its
description carries a 2026-08-25 CORRECTION that explicitly SUPERSEDES its own
comments below, and a retraction naming a dead section. Read the description
top-down and stop trusting anything below the correction. `bd comments
astro-plan-mg6h8` has 13 comments; the J04 prep plan records the
2026-08-24T20:06:35Z comment as the live recipe -- that is a conflict with the
description's own claim to supersede the comments, and **I could not resolve
which is authoritative without the host.**

`astro-plan-ldj0v`'s description asserts the host is a serial resource. That
is refuted. The bead should not be read as current on that point.

---

## 2. SPEC STATE

### 2.1 The standing conclusion -- verified, still holds

A four-batch per-spec correctness sweep (`astro-plan-drvbd`, `cj7ux`, `byb4b`,
`2andq`, all P1 in_progress) reached this verdict, which is carried forward in
substance: **behaviour, requirements and domain rules held up wherever tested,
but no concrete identifier in `specs/` can be trusted** -- paths, line numbers,
crate names and migration filenames rot. This is a MECHANICAL consequence, not
carelessness.

Verified 2026-08-25: `grep -n "^exclude:" .pre-commit-config.yaml` (exit 0)
returns line **16**, and the pattern excludes `specs/` wholesale alongside
`.specify/`, `.claude/`, `.agents/`, `.codex/`, `.mcp.json` and
`apps/desktop/src/bindings/`. Nothing checks 503 tracked spec files
(`git ls-files specs | wc -l`, exit 0). The config's own comment records the
measured cost of un-gating: trailing-whitespace would rewrite 34 files / 129
lines, end-of-file-fixer 5 files, and `typos` reports 20 findings across 14
files, 9 of them in `tasks.md` files that the `speckit-tasks-guard` hook denies
writing under an active beads workspace. So the fix is not a one-liner.

### 2.2 `astro-plan-2mfx6` -- the recurrence-stopping mechanism -- DID NOT LAND

`astro-plan-2mfx6` (P1) is **still open**, last updated 2026-08-24, 2 comments,
0 dependencies, no PR. Its title is the fix: "make specs/ checkable -- a
path-existence check plus narrowing the wholesale exclude". Its description
records that the sweep's own verdict was that its four filed beads are NOT the
fix with the widest reach. This gate is.

Verification that nothing landed: `ls -1 scripts/` (exit 0) lists 42 entries
and contains no spec path checker (`check-db-boundary.sh`,
`check-dead-callers.sh`, `check-generated-drift.sh`,
`check-github-not-ignored.sh`, … -- none for specs). A combined
`grep -rln 'spec.*path.*exist|specs-path|spec-paths' scripts/
.pre-commit-config.yaml justfile` **exited 1 (no match)**. The `exclude:` line
is unchanged.

**Consequence for the new owner: every identifier you read in `specs/` must be
re-verified against code before you act on it, and this will remain true until
`2mfx6` lands.** That is the repo-hygiene item that unblocks the other 502 spec files.

### 2.3 What the spec tree actually contains, measured 2026-08-25

- 60 numbered spec directories (`ls -1d specs/0* specs/1*`, exit 0). Numbers
  run 001-062 with gaps: **034, 059 and 060 have no directory** (SPEC_STATUS
  states their absence is correct), and both `037-e2e-integration-testing` and
  `037-ipc-wrapper-removal` exist -- a duplicated number.
- Plus `specs/tiny/` with 12 documents (`git ls-files 'specs/tiny/*'`, exit 0).
- Plus 7 top-level markdown files (`git ls-files … | grep -c`, exit 0).
- 503 tracked files total; 123 under `specs/*/contracts/*`; 23 under
  `specs/*/checklists/*` (`git ls-files`, exit 0 each).

The brief's "69 spec entries" corresponds to 60 dirs + 7 top-level md + `tiny`
+ 1 (most plausibly the sweep counting `tiny` documents or the duplicate 037
differently). **I could not reproduce 69 exactly** and record 60+12+7 as the
measured decomposition. The sweep's coverage claim of "69/69" therefore cannot
be mapped onto the tree with confidence.

### 2.4 Status per spec -- from `specs/SPEC_STATUS.md`, which is itself dated

`specs/SPEC_STATUS.md` is the reconciled index and states outright that the
`Status:` line inside each individual `spec.md` "had drifted badly -- most still
read Draft despite shipping". **Do not read individual spec.md status lines.**

`grep -c '^| [0-9]' specs/SPEC_STATUS.md` -> **63 rows** (exit 0). Marker
distribution (`grep -o | sed | sort | uniq -c`, exit 0), collapsing the
free-text variants:

- ✅ Implemented / Closed / Complete: ~31 rows (20 plain "✅ Implemented",
  3 "closed 2026-07-03", plus per-spec variants such as 007 39/42, 009 33/44,
  012, 037, 041).
- 🟡 Partial / closeout-ready: ~13 rows, each naming its remainder (e.g.
  "42/51 -- small remainder open", "41/46", "92/96 -- 4 honest partials",
  "35/37, 2 honest deferrals", "Partial, US2 not built", "Partial, US3 not
  built").
- 🔴 Superseded: 8 rows -- 005 -> 041, 013 -> 035, 010 -> 056, 030 -> 032 (the
  banner repoint landed as PR #1753 across 26 spec files), one -> 027, plus
  "Superseded" and "Obsolete as a work queue" unqualified.
- ⚪ Not started: 1 row.
- 📄 Specified / plan-of-record only: 2 rows.

The index's own reconciliation dates are staggered and it says so: full sweep
2026-06-23, updates 2026-07-03/04/09/19, and a **partial** 2026-08-24 gap-fill
that added rows for 052/053/054/055/057 and verified ONLY those five against
`origin/main`. **Every other row still dates from its own note and may have
drifted.** Three doc-correction PRs landed 2026-08-24 (#1749 corrected nine
understated statuses, #1746 indexed five unlisted specs and repointed dead
migration citations, #1754 repointed live pointers naming deleted crates and
migrations), so the index is better than it was but current only in the rows named below.

Specs with known substantial open work, from the release-blocker bead and the
index: spec 010 guided-first-project-flow had **zero production code** (its
successor is `crates/app/core/src/onboarding.rs` via spec 056); spec 052's
paper is inverted relative to shipped code; specs 015/023/061 are
checkbox-less; **172 of 242 spec task ids are named nowhere** (recorded on
`astro-plan-78kk`); spec 043 names a dead `--alm-*` token namespace, 4 of 6
themes and the wrong theme storage key (`astro-plan-tf9fn`); spec 062 session
heterogeneity is mid-flight (`ic9h` family, `ps48`, `b4ng`,
`ic9h.20` blocked); spec 017 has a reopened-plan dead end (`astro-plan-4puw`).

### 2.5 UNEXAMINED -- no sweep ever reached these

Stated plainly so the gap is not mistaken for coverage. These files exist and
were never assessed by any spec sweep, and not by this document either beyond
confirming existence and size (`wc -l`, exit 0):

| File | Lines |
|---|---|
| `specs/AGENTS.md` | 19 |
| `specs/CLAUDE.md` | 228 |
| `specs/PENDING_REVIEW_QUESTIONS.md` | 454 |
| `specs/PENDING_IMPL_QUESTIONS.md` | 183 |
| `specs/GRILL_DECISIONS_2026-05-21.md` | 1463 |
| `specs/SPECKIT_PASS_2026-05-20.md` | 528 |

Also unexamined: **all 123 documents under `specs/*/contracts/`** and **all 23
under `specs/*/checklists/`**. `specs/PENDING_REVIEW_QUESTIONS.md` (454 lines)
and `PENDING_IMPL_QUESTIONS.md` (183 lines) are by their titles lists of open
questions -- they may contain unanswered product decisions that exist nowhere
in beads. `GRILL_DECISIONS_2026-05-21.md` at 1463 lines is the largest
unexamined document in the tree and is by its name a decision record. **Reading
these three is a candidate for the first spec-side task.**

`specs/tiny/` (12 documents) has been touched at least once (#1744 corrected
`settings-key-canonicalization`) but was not systematically swept.

---

## 3. JOURNEY VALIDATION STATE -- the live front, the most perishable knowledge

**The headline: 18 journeys exist; ZERO durable run records exist.**
`find docs/journeys -path '*/runs/*' | wc -l` -> **0** (exit 0). Every J\* dir
holds only `journey.md` (checked for J07, `ls -1`, exit 0). The product is
feature-complete enough to validate and **has never been validated end to
end**. `astro-plan-7z070` (open P2, "merge: journey run records for J06/J07
(blocked runs)") implies two run records exist on an unmerged branch -- they are
recorded as BLOCKED runs, i.e. the driver could not execute JS (cf. merged
#1751 "record blocked validation runs -- driver cannot execute JS"). So even
those are not passes.

`ls -1 docs/journeys/` (exit 0): 18 journey directories J01-J18, plus
`FORMAT.md`, `INDEX.md`, `README.md`, `journeys.py`.

### 3.1 Per-journey status, 2026-08-25

| J | Title | Offline prep | Driven | Outcome |
|---|---|---|---|---|
| J01 | first-run setup -> Data Sources | **COMPLETE**, 789-line plan | driving at time of writing (`xjow5` in_progress) | no run record |
| J02 | inbox ingest -> review -> reclassify -> confirm -> move | **COMPLETE**, 680-line plan | not driven (`yepf9` open) -- but bridge facts were measured on instance 2/port 9224 under this bead | no run record |
| J03 | inbox confirm -> catalogue in place | **COMPLETE**, 352-line plan | driving (`7dmmk` in_progress) | no run record |
| J04 | sessions review (derived) | **COMPLETE**, 750-line plan -- see correction below | not driven (`yle7a`/`tojh4` open P2) | no run record |
| J05 | project lifecycle | not started (`nhna3` open) | no | -- |
| J06 | cleanup scan -> review -> apply | **COMPLETE**, 644-line plan | not driven (`14fjz` open) | a BLOCKED run exists unmerged (`7z070`) |
| J07 | archive -> delete | not started (`s23mm` open) | **the only journey ever driven before 2026-08-25** -- reached 1 of 9 steps | BLOCKED run, unmerged (`7z070`) |
| J08 | calibration ingest -> masters -> matching | not started (`dfmsv`) | no | -- |
| J09 | targets & planning | not started (`2ojlm`) | no | -- |
| J10 | settings, appearance, i18n | not started (`tehzy`) | no | -- |
| J11 | mistake recovery | not started (`3mqwh`) | no | -- |
| J12 | failure & refusal handling | not started (`xgb3x`) | no | -- |
| J13 | audit & activity investigation | not started (`pwq42`) | no | -- |
| J14 | target-first project start | not started (`usadg`) | no | -- |
| J15 | equipment & observing-site setup | not started (`7r3m4`) | no | -- |
| J16 | keyboard-first navigation | not started (`j9mgr`) | no | -- |
| J17 | software update & install | not started (`l3xbd`) | no | -- |
| J18 | onboarding orientation | not started (`6wd89`) | no | -- |

**Correction to the commissioning brief:** it says "J04 prep was running".
`<scratch>/journey-prep/J04/drive-plan.md` is 750 lines and ends with a
finished zero-result sanity section (it validated its own negative searches
against a known-matching case, `rg -c 'Confirm' SessionDetail.tsx` -> 8, and
records avoiding `grep | head` because that masks no-match exit status).
`astro-plan-ba3yl` is still marked in_progress but **the artifact looks
complete**. Verify before redoing it. This is exactly the failure mode a system
move causes: bead status says one thing, the out-of-repo artifact says another.

J04's plan also records a live doc/product mismatch worth carrying:
`common_reveal_windows` / `_macos` / `_linux` are **absent** from
`messages/en-GB.json`; the real keys are `reveal_label_*`.

J01's plan records another: **the product has 8 wizard steps, not the 7 the
journey describes** (`SetupWizard.tsx:86-130`; the extra step is Theme). Filed
as `astro-plan-ko0tv` (P3).

`astro-plan-7wc6f` (P2): J17 claims `status: active` with zero run records,
which `FORMAT.md` does not permit.

### 3.2 The Windows working environment -- and an unresolved port conflict

Host **<journey-host>** over ssh. Per-instance directories `C:\pv-multi\i{1,2,3}\`
each with its own `webview2\EBWebView`, `app.db`, `app.db-wal`, `appdata\`,
`localappdata\`. Instances are launched via `schtasks /IT` into **console
session 1** (not the ssh session -- see trap T5, this is a safety requirement,
not a convenience). Isolation is real: three distinct WebView2 trees, and a
unique localStorage nonce written to instance 2 read back `null` from 1 and 3.

**Three sources disagree about the bridge ports and I could not resolve it
without the host, which I am forbidden to touch:**

- The commissioning brief: **9223/9224/9225**, and states the
  `9223+(N-1)*100` scheme exists only in unmerged PR #1757.
- `astro-plan-dfv3s` (written 2026-08-25, describing the run in progress):
  "ports **9223/9323/9423**".
- `astro-plan-95zbx` (measured): the plugin's
  `discovery.rs:16-20 find_available_port(bind_address, base_port)` scans **100
  ports upward from the base**, 9223 is only the default `base_port`
  (`config.rs:22`), `Builder::base_port()` is a setter, and the app calls
  `Config::new(bind)` (`apps/desktop/src-tauri/src/lib.rs:136`) so it pins
  nothing. The plugin logs the port it took (plugin `lib.rs:223`, "on
  {bind_address}:{port}").

The mechanism in 95zbx implies **9223/9224/9225** for three instances sharing a
base, which supports the brief. The `+100` spacing in dfv3s is what #1757
introduces. **Resolve this by reading the plugin's own log line on each
instance -- never assume.** Getting it wrong means driving the wrong instance,
which is a confident-wrong-answer failure of the same family as Part 4.

Other environment facts: the single-instance guard is already bypassable via
the `e2e` feature plus `PV_E2E_INSTANCE_ID` (`lib.rs:176`), with a regression
test proving the env var alone cannot disable it in a non-e2e build. Vite's
`strictPort` 5173 binds only in dev, not in a built binary. The MCP client
`@hypothesi/tauri-mcp-server@0.11.2` reads `MCP_BRIDGE_HOST` and
`MCP_BRIDGE_PORT` from the environment. `.mcp.json` declares 5 servers:
`context7`, `mcp-package-version`, `playwright`, `repomix`, `tauri` (python
read of the file, exit 0). Merged #1748 added the `tauri` server; merged #1752
documented the `TAURI_CONFIG` build required for `execute_js` to return.

`docs/journeys/README.md:44-45` still declares the `desktop-ui` profile
`exclusive: true` with the rationale "only one validator can hold the Windows
checkout/app process at a time". **That rationale is FALSE.** Correcting it is
`astro-plan-95zbx`; adding a per-journey exclusivity field to `FORMAT.md` is
`astro-plan-a9rjq`.

`astro-plan-iizrh` (P2) records the host as not provisioned (no node, etc.) --
that predates the current working setup and may be stale; **untested**.

### 3.3 macOS lane

Proven viable. The `desktop-ui-macos` profile is now declared on **all 18
journeys** via merged PR #1756 (`gh pr list --state merged`, exit 0, merged
2026-08-25T05:37:31Z). Four P1 macOS drive beads exist (J01/J02/J03/J06).

**The macOS lane has one known blocker**: `astro-plan-qvmqq` (P1, in_progress)
-- WebKit localStorage is **shared** across concurrent app instances on macOS,
defeating instance isolation. Concurrency on macOS is therefore not safe until
that lands, even though concurrency on Windows is proven. And when verifying
that fix, do not use `appDataDir()` (trap T3) -- it will report one shared path
on macOS regardless and you will conclude the fix failed.

---

## 4. THE TRAPS

Each was paid for with real hours. The first four are the dangerous ones
because they produce a **confident wrong answer rather than an error** -- no
exception, no warning field, no non-zero exit. A validator hits one of these
and files a bug report against working code, or discards a valid run.

### T1. `get_window_info` silently answers about the wrong window
Bead `astro-plan-ts2r6` (P1), verified on the host against bridge 9223.
`get_window_info` with `{"windowLabel":"splash"}` returned `success:true` with
the MAIN window's geometry (1372x1136), even though `list_windows` reports one
window labelled `main`. It never errors on an unknown label because it never
reads `windowLabel`: `tauri-plugin-mcp-bridge-0.11.2/src/websocket.rs:266-268`
reads `args.windowId`, falls back to `'main'`, and
`resolve_window` (`src/commands/list_windows.rs:159-166`) errors only when the
*resolved* label is absent. `execute_js` by contrast reads `args.windowLabel`
(`websocket.rs:287-290`). **The two calls take differently-named window
arguments.** The driver recipe on `astro-plan-mg6h8` used the `windowLabel`
spelling for `get_window_info`, so every such call reported the main window
regardless of what was asked. Not a PlateVault defect -- upstream plugin API
inconsistency. Rule: `windowId` for `get_window_info`, `windowLabel` for
`execute_js`.

### T2. `execute_js` returns `data:null` for a statement list, so a working probe reads as a failure
Recorded on `astro-plan-mg6h8`, measured on instance 2 / port 9224. Two facts:
(a) the argument is **`script`, not `code`** -- `{windowLabel, code}` returns
`{success:false, error:"Missing script argument"}`; (b) **a multi-statement
script returns `data:null` even when it succeeded.** Sending
`localStorage.setItem(k,v); JSON.stringify({...})` returned
`{success:true, error:null, data:null}` -- yet the write HAD happened, proven by
a separate single-expression read. It returns only an expression's value. It
DOES await promises. Rules: wrap every probe in a value-returning IIFE
`(() => { ...; return JSON.stringify(x); })()`; judge success by `data`, never
by `success:true`; **never record a negative observation from an unwrapped
script.** This is precisely how a validator concludes "the app does not
respond" when it does.

### T3. `appDataDir()` gives a FALSE CONTAMINATION signal
Bead `astro-plan-vi9sp` (P2), measured by the J01 drive validator on the
Windows host 2026-08-25. `appDataDir()` returns the **same path on every
concurrent instance** --
`C:\Users\<user>\AppData\Roaming\dev.astro-plan.astro-library-manager` -- because
that Tauri API derives from the bundle identifier and ignores the per-instance
isolation environment entirely. The instances ARE isolated (three distinct
WebView2 trees; a nonce written to instance 2 read `null` from 1 and 3). The
shared directory is stale and unused: its `alm.db` was last written 2026-07-03.
Wrong in the dangerous direction -- it would make someone discard valid journey
runs or "fix" code that already works. **And it is the mirror of a real bug:**
`astro-plan-qvmqq` is a real shared-storage defect on macOS; anyone
verifying that fix with `appDataDir()` will see one shared path on macOS too,
conclude the fix failed, and keep changing correct code.

### T4. The `[token-savings]` Bash wrapper rewrites literals inside command output
Memory note `token-savings-wrapper-corrupts-grep-output.md`. The wrapper
**rewrote a literal search term inside `rg` output**, so a correct command with
an honest zero exit code returned a false zero. Mitigation used throughout this
document: redirect output to a file and read the file back, never trust inline
wrapped output for anything literal. The wrapper also truncates to
first-15/last-25 lines and spills the rest to
`~/.local/state/agentic-tools/token-savings/spill/` -- a middle section can
vanish silently.

### T5. Trashing a file over ssh on the Windows host PERMANENTLY DELETES it
Bead `astro-plan-fg3se` (P1, tagged SAFETY), measured on <journey-host>.
Identical code --
`Microsoft.VisualBasic.FileIO.FileSystem::DeleteFile(path,'OnlyErrorDialogs','SendToRecycleBin')`
-- behaves differently by session. Over ssh (session 0) the file leaves disk and
**no `$I` record appears anywhere under `C:\$Recycle.Bin`**; all three per-SID
directories held 0 `$I` records afterwards. Run in session 1 via
`schtasks /IT`, same user (`DESKTOP\Sjors`, SID `S-1-5-21-...-1001`), three
`$I` records appeared naming the correct original paths. Two consequences:
(1) a trash assertion driven from ssh **cannot fail for the right reason** and
is vacuous -- J06 and J07 both depend on this distinction; (2) it is
**irreversible**, and the host holds 971 MB of the owner's real frames under
`C:\Temp\pv-*` (`astro-plan-9turs`) that must never be reached. Rule: never
perform a filesystem-destructive operation from the ssh shell; launch the app
in session 1 and drive it through the bridge so the app deletes the file.
Separately verified: the Recycle Bin is SHARED across app instances.

### T6. Comparing localStorage CONTENTS between instances proves nothing
All instances show identical default first-run state, so equal contents is
consistent with both isolation and contamination, and the test is
non-discriminating in **both** directions. Only a **unique nonce** written to
one instance and read back from the others discriminates. That is the test that
actually established isolation.

### T7. A clean commit is not evidence pre-commit ran
Bead `astro-plan-vi96h` (P1), memory note
`green-commit-does-not-prove-precommit-ran.md`. `core.hooksPath` is redirected
machine-wide. Verified 2026-08-25: `git config --get core.hooksPath` ->
`/usr/local/amazon/var/git-defender/hooks` (exit 0);
`git config --global --get core.hooksPath` -> exit 1, so it is set at
local/system scope, not global. With hooks redirected, **zero repo hooks run**
and the commit is clean for the wrong reason. `scripts/precommit-verify.sh`
exists (`ls -1 scripts/`, exit 0) and fails a zero-hook run. Rule: demand a
direct gate exit code; never infer gate execution from a clean commit. Also
standing: never `--no-verify`.

### T8. A convention-shaped search cannot find the exception
Memory note `naming-convention-search-hides-the-exception.md`. A search for
`ORDER BY *_at` returned **zero hits on a column named `at`** -- and that column
was the live defect. Rule: run the looser pattern too and diff the results.
Validate any zero-result search against a
case known to match first (the J04 plan does exactly this, and records `rg -c
'Confirm' SessionDetail.tsx` -> 8 as its positive control). Also: `grep | head`
masks no-match exit status, and `\b` is absent from POSIX ERE.

### T9. Seven shapes of vacuous test have shipped here
The memory note is `six-shapes-of-vacuous-test.md` and documents **six**; the
brief names a seventh. All were found by **mutating production code**, never by
reading the test.

1. **Empty joined table.** The SQLite planner skips `json_each` entirely when
   the joined table has no rows, so malformed JSON is never parsed and the
   guard is never exercised.
2. **Pruned join.** Selecting a single column lets the planner drop the join --
   same effect.
3. **Short-circuit before the arm.** `detect_master` returns the first detector
   reporting evidence, and Siril reported whenever `IMAGETYP` parsed, so two
   tests named for the PixInsight arm never reached it.
4. **Unfailable by construction.** A `does_not_panic` test with nothing that
   could panic.
5. **Wrong half reverted.** Reverting only `dispatch.rs` left a containment
   test passing via the gate in `loop_.rs`; two tests were revert-red on
   *different* halves, so a single-half revert proved nothing. (PR #1738)
6. **Fixture supplies what production should derive** -- the sneakiest. An
   end-to-end test injected `camera_body_id` by direct SQL `UPDATE`, so nulling
   the real derivation at `ingest_sessions.rs:367` left **all 11 integration
   tests green**. The assertion never reached the code under test. (PR #1742)
7. **Platform-gated early return.** `if !cfg!(windows) { return; }` -- the body
   never executes off Windows, and CI's primary lane is not Windows. Verified
   present 2026-08-25 at two sites (`rg -n 'if !cfg!\(.*windows'`, exit 0):
   `crates/app/core/src/first_run/tests.rs:188` and
   `crates/app/projects/src/source_view_generate/generate.rs:413`. **I did not
   establish that the memory note's author intended these two as the seventh
   shape** -- the note documents six and stops. Treat shape 7 as a live pattern
   to check rather than a closed finding, and note the second site is in
   `generate.rs`, i.e. production, not a test file.

Shapes 1-4 are "the assertion cannot fail"; 5-7 are "the assertion never
reaches the code", which **reading the test will not reveal** -- the test looks
thorough and asserts real values. How to apply: require a revert/mutation
result **per test**, and mutate the *specific line* the test claims to cover,
not the whole change. For an end-to-end test, check whether the fixture hands
over the exact value production is supposed to compute. When a change spans
layers, revert each layer separately.
The accepted exception, standard set on PR #1737 and since verified on five
PRs: a test that legitimately cannot fail at base is acceptable when it guards
a future over-correction, **but only** if you prove it discriminates by
substituting a WRONG implementation and showing it fails, and you disclose it.
An undisclosed unfailable test is a blocking defect; a disclosed one with a
mutation proof is not.

### T10. MCP servers bind at session start
A session that predates a `.mcp.json` change has no such tool **regardless of
what is on disk**. If a tool appears missing, check when the session started
before debugging the config. Relevant here because #1748 added the `tauri`
server on 2026-08-24: any session started before that has no Tauri driver.

### T11. Bead status and out-of-repo artifacts diverge
Not in the brief's list; found while writing this. `astro-plan-ba3yl` (J04
offline prep) is `in_progress`, yet
`<scratch>/journey-prep/J04/drive-plan.md` is a finished 750-line plan
with a closing sanity section. `astro-plan-ldj0v`'s description still asserts
the host is a serial resource, which is refuted. `astro-plan-mg6h8`'s
description explicitly supersedes its own comments while the J04 plan cites a
comment as the live recipe. **Rule: on this project, read the artifact and the
bead, and when they disagree prefer the artifact with a date and a command in
it.**

### T12. `docs/development/windows-journeys/` manufactures false FAILs
Beads `astro-plan-yhx5v` (P1) and `astro-plan-u1aax` (P2).
`docs/journeys/README.md` points validators at this directory as a canonical
aid carrying "exact click sequences and troubleshooting". **Three of three
files checked are defective.** `journey-03-inbox-catalogue-in-place.md:82-88`
is wrong in a way that produces a **false FAIL**: the destructive control is
gated on `hasDestructive` (`PlanPanel.tsx:199-201,401`), so the journey
document's S3 negative is correct and the aid contradicts it.
`journey-01-first-run-setup.md` is stale in at least 5 expectations and names
`data-testid`s that do not exist -- its two cited tokens `e2e-path-input-<kind>`
and `e2e-add-path-btn-<kind>` (`:70-74`) are absent from
`apps/desktop/src` (`rg -c e2e-path-input` exits 1, sanity-checked against
`rg -c cleanup-scan-btn … --glob *.tsx` -> 4 files; filed as
`astro-plan-npts7`). Provenance is the strongest evidence: 10 of 11 files have
had no content change since 2026-07-17 (journey-02 since 2026-07-24), while
`docs/journeys/` is on v6-v9 with delta logs through 2026-08-24; the
2026-08-22 touch on all 11 is `13353a066` (MCP gating, #1701), not a refresh.
**This already cost a full fix-and-review cycle**: PR #1755's validator
reported "no UI response whatsoever" when the response existed and only the
selector was wrong, producing a bug report wrong in three separate places.
Rule: derive selectors from React source, as all five drive plans did. Do not
open this directory.

### T13. Other traps carried in the memory directory
Not verified by this unit. Each has a note in
`<agent-memory>/`.
Named so they survive the move: `shared-cargo-target-dir-fakes-compile-errors`
(a concurrent agent's build erases branch-new symbols -- use `CARGO_TARGET_DIR`
rather than debugging the import), `merge-tree-conflict-is-not-unlanded-work`
(squash merges fool `merge-tree` and `git cherry`; only `--is-ancestor` on the
squash sha is reliable), `primary-checkout-is-stale`,
`llvm-cov-job-needs-node-deps`, `workspace-cargo-build-is-9gib`,
`jq-keys-on-first-record-loses-fields`,
`coderabbit-review-finished-is-not-evidence`,
`claude-settings-is-chezmoi-managed` (removed hook entries return on the next
`chezmoi apply`, and a registration whose script was deleted kills every Bash
call in every agent -- **directly relevant to a system move**),
`never-relay-an-unverified-child-figure`, `dgit-cannot-see-tmp`,
`endpoint-security-amplifies-parallel-file-churn`,
`journey-host-is-not-actually-serial`, `release-requires-explicit-approval`.

---

## 5. WHAT THE NEXT ACTOR SHOULD DO FIRST

Ordered. "Silently" below means no error and no failing check.

1. **Copy §0's paths off the old machine before anything else.** 3215 lines of
   source-derived drive plans, a working bridge client, and 40 memory notes are
   outside git. Nothing else in this list is recoverable-cheap by comparison,
   and this step has a deadline the others do not. *Breaks silently:* the work
   simply is not there later and nobody knows it existed.
2. **Re-read `astro-plan-vano` (72 comments, newest first) and
   `astro-plan-ip9p7`.** They are the project's own handover anchors and carry
   the standing holds. This document supplements them; it does not replace
   them.
3. **Do not touch PR #1393.** Nothing about a green CI, a closed bead, or a
   passing journey authorizes it. Only the owner does. *Breaks silently:*
   merging a held release is unrecoverable.
4. **On the new machine, check `git config --get core.hooksPath` and run
   `scripts/precommit-verify.sh` once.** If hooks are redirected there too,
   every clean commit you make is uninformative (T7). Do this before writing
   code, not after.
5. **Resolve the bridge port question by reading the plugin's log line**, not
   by assuming (§3.2). Three sources disagree. Everything downstream --
   every drive, every probe -- targets a port.
6. **Land `astro-plan-2mfx6` (make `specs/` checkable).** Nearly-specified,
   barely-started: the diagnosis is complete and the measured cost is written
   into `.pre-commit-config.yaml`'s own comment, but no code exists and the
   `exclude:` line is untouched. Until it lands, every path, line number, crate
   name and migration filename in 503 spec files is untrustworthy and will rot
   again. *Breaks silently:* it already has, repeatedly.
7. **Read `specs/PENDING_REVIEW_QUESTIONS.md` (454 lines),
   `PENDING_IMPL_QUESTIONS.md` (183) and `GRILL_DECISIONS_2026-05-21.md`
   (1463).** These are unexamined and, by their titles, hold open questions and
   decisions that may exist nowhere in beads. This is the largest
   unknown region in the project. *Breaks silently:* an unanswered product
   question is invisible until someone builds the wrong thing.
8. **Decide `astro-plan-9turs` with the owner, and do not let any agent near
   `C:\Temp\pv-*` first.** One file has no original. Combined with T5, one
   careless step is unrecoverable loss of the owner's astrophotography data.
   Also finish the census for other files of that shape -- it is unstarted and
   needs the host. *Breaks silently and irreversibly.*
9. **Resume journey validation.** Nearly done: J01/J02/J03/J04/J06 have
   complete plans and the concurrency capability is proven on hardware. Barely
   started: 13 of 18 journeys have no prep, and **all 18 have no run record**.
   Before driving, internalise T1, T2, T6 and T12 -- three of the four
   confident-wrong-answer traps live in the drive loop, and T12 has already
   produced one bogus bug report at a cost of a full fix-and-review cycle.
   Land or close `astro-plan-a9rjq` and `astro-plan-95zbx` so
   `docs/journeys/README.md:44-45` stops asserting a false rationale.
10. **Fix `astro-plan-qvmqq` before running the macOS lane concurrently**, and
    do not verify the fix with `appDataDir()` (T3). The macOS profile is now on
    all 18 journeys (#1756) so the lane looks ready and is not.
11. **Adjudicate the four overlapping journey-bead families (§1.5) and the two
    duplicate pairs.** Do not close anything without owner authority, but a new
    owner reading 702 beads will otherwise plan the same 18 journeys four
    times.
12. **Triage the 3v3r audit tree by consequence, not by id.** 244 of 756
    non-closed beads sit in it. Start with the Tier-1 durability set in §1.3
    (`3v3r.3.33`, `3v3r.4.30`, `3v3r.6.22`, `3v3r.4.20`, `3v3r.8.22`) -- those
    touch records the filesystem cannot re-derive, which is exactly what the
    constitution's Principle V protects. `3v3r.19.10` (release profile omits
    `overflow-checks` in every shipped binary) and `3v3r.17.10` (unpinned
    action from an archived repo holding a GitHub App private key on
    `pull_request_target`) are cheap and should go early.
13. **Land the three open drafts (#1757, #1758, #1759) or close them.** They
    have merge beads (`zhds2`, `9labh`, `cf5ec`) and #1757 is the port scheme
    everything else keeps tripping over.
14. **Defer the entire agent-tooling theme** (§1.3, ~24 P1 beads about
    worktrunk, rules-eval, pr-create-guard, orchestrate). None of it is product
    behaviour, and a system move invalidates parts of it anyway. The one
    exception is `astro-plan-p7xpf` -- PR #1706 drops the blocks-edge and
    PR-trailer gates and wants an explicit decision before it lands.

### What is nearly done vs barely started

**Nearly done:**

- The release. Green, held by policy only.
- J01-J04 and J06 prep. Complete plans, out of repo.
- Concurrent multi-instance driving. Proven on hardware, mechanism unmerged in
  #1757.
- The macOS lane. Declared everywhere, one blocker.
- The spec-status index. Three correction PRs landed 2026-08-24.

**Barely started:**

- Journey run records. **Zero**, out of 18.
- 13 of 18 journey preps.
- The `specs/` checkability gate. Diagnosis only, no code.
- The `C:\Temp\pv-*` census beyond the 22 known files.
- Spec 010. Zero production code, superseded by 056, but its FR-009 carries
  forward.
- Spec 062 session heterogeneity.
- The 123 contracts and 23 checklists documents. Never read by anyone.

---

## 6. WHAT THIS DOCUMENT DID NOT ESTABLISH

Stated so no gap looks like coverage.

- **Anything on the Windows host.** This unit was forbidden to touch
  <journey-host> (a validator was driving it). Every host fact here is quoted
  from a bead or a drive plan, not re-measured. That includes the port
  question, the 971 MB inventory, and the current state of the J01/J03 drives.
- **The 69-entry spec figure.** Measured 60 dirs + 12 `tiny` docs + 7
  top-level md; could not reproduce 69 exactly.
- **The "84 Expects" figure for J01.** Measured 109 rows matching
  `^| *E[0-9]`, 14 unique `E<n>` ids.
- **Whether shape 7 of the vacuous-test taxonomy is the intended seventh.** The
  memory note documents six. Two `if !cfg!(windows)` sites verified present.
- **Whether any P2/P3/P4 bead outside the clusters named in §1.3 is still
  live.** 364 P2 + 156 P3 + 56 P4 open beads were counted and clustered, not
  read.
- **`specs/AGENTS.md`, `specs/CLAUDE.md`, `PENDING_REVIEW_QUESTIONS.md`,
  `PENDING_IMPL_QUESTIONS.md`, `GRILL_DECISIONS_2026-05-21.md`,
  `SPECKIT_PASS_2026-05-20.md`, all 123 `contracts/` documents and all 23
  `checklists/` documents.** Existence and line counts only. No sweep has ever
  read them.
- **Per-spec verification.** §2.4 reports what `SPEC_STATUS.md` claims. Only
  052/053/054/055/057 were ever verified against `origin/main`, on 2026-08-24,
  by someone else.
- **Whether `astro-plan-iizrh`'s "host not provisioned" finding is still
  true.** It predates the working multi-instance setup.
- **Whether `<scratch>/pv-bringup`'s 264 MB is regenerable** without
  the host.

Evidence files for every command above sit in
`<scratch>/handover/_ev/`. All commands reported exit 0 except the two
noted as exit 1 (`git config --global --get core.hooksPath`, and the
spec-path-check grep whose exit 1 is itself the finding).
