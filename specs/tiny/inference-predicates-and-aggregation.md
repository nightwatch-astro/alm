# TinySpec: Predicates and aggregates claim only what their evidence supports

**Branch**: fix/infer1-evidence-matching
**Date**: 2026-08-23
**Complexity**: 3 (TinySpec route, speckit-bugfix)
**Findings**: astro-plan-3v3r.2.30, .2.31, .2.35, .2.25, .13.28 (LIVE);
.13.37 (LIVE as a duplicated rule, no behaviour change)

## What

Six inference sites answered a question their inputs could not settle: a
substring test stood in for a token test, a name tail stood in for an
extension, a header count present in the input was reported as absent, and one
status compared two candidates that were never in competition.

| Site | Predicate today (base 660d665ec) | What the evidence supported |
|---|---|---|
| `crates/calibration/master-detect/src/lib.rs:167` `path_looks_like_master` | `name_lc.contains("master") \|\| path_lc.contains("master")` over the whole string | a `master`-prefixed token bounded by `/`, `\`, `_`, `-`, `.` or space |
| `crates/calibration/master-detect/src/lib.rs:138` `parse_frame_type` | ordered `str::contains` chain, so `dark_flat` hits the `dark` arm | the first matching whole token, `darkflat` or adjacent `dark`+`flat` first |
| `crates/calibration/master-detect/src/pixinsight.rs:49` `PixInsightDetector::detect` | `stack_count_evidence: false` unconditionally; `is_master` from naming only | a present `STACKCNT`/`NCOMBINE` is header evidence and decides `is_master` |
| `crates/calibration/core/src/ranking.rs:238` `suggest_status` via `crates/app/calibration/src/matching/suggest.rs:60` | `matches[0]` against `matches[1]` on a list ranked per kind but unordered across kinds | statuses reduce within one calibration kind, then combine |
| `crates/workflow/artifacts/src/watcher.rs:104` `extension_allowed` | `file_name.to_ascii_lowercase().ends_with(ext)` | `Path::extension()` compared against each allowlist entry |
| `crates/workflow/artifacts/src/attribution.rs:66` `reattribute_candidates` | doc claims the same-tool rule; the rule lived in the caller at `crates/app/lifecycle/src/artifact/launches.rs:54` | the function that documents a rule owns it |

## The rule (decision)

**A classification or aggregation result must be derivable only from the
evidence it consumed: predicates match delimiter-bounded tokens and real
extensions, and statuses reduce within a calibration kind.**

Three consequences bind every site above.

| Consequence | Applied at |
|---|---|
| A word is present only as a token, never as a substring. Tokens are split on `/`, `\`, `_`, `-`, `.` and space, on every platform. | `path_looks_like_master`, `parse_frame_type` |
| A file extension is what `Path::extension()` returns, not a tail of the name. An allowlist entry is compared with its leading `.` stripped, so a dotted and a dotless list behave alike. | `extension_allowed` |
| Two confidences are comparable only inside one calibration kind. | `multi_kind_suggest_status` |

`master` is accepted as a prefix followed by a frame word (`masterDark`,
`masterDarks`, `masterFlat`), because that is the WBPP convention. Whole-token
equality alone would turn the primary supported case into a false negative;
prefix matching alone would keep `masterclass_notes/` a master store. Both
forms are asserted.

## Per-finding verdict on `fix/infer1-evidence-matching`

Line references are the post-fix head of this branch; the "today" column in
`## What` is base `660d665ec`.

| Finding | Verdict | Reachable in production | Evidence read at this head |
|---|---|---|---|
| .2.30 | LIVE | **Yes.** `detect_master` is on the scan path, so any library with a folder whose name merely contains `master` had every descendant file flagged a calibration master | `lib.rs:180` `path_looks_like_master` now splits both separators and tests `is_master_token` per segment (`lib.rs:194`) |
| .2.31 | LIVE, in part | **Partly.** The proven defect is the round trip: `FrameType::DarkFlat.as_str()` is `"dark_flat"`, and feeding it back returned `Dark`, so classification was not idempotent. Whether a capture tool writes that spelling into `IMAGETYP` is not established, so the header-side impact is unproven | `lib.rs:142` `parse_frame_type` tokenizes and matches whole tokens. `infer_frame_type_from_path` (`pixinsight.rs:90`) still reads a frame word out of a target name and is **not** fixed here — filed as astro-plan-qs26q |
| .2.35 | LIVE | **Yes.** A PixInsight/WBPP file whose `IMAGETYP` was absent reported `stack_count_evidence: false` with `STACKCNT` present in the same input, and `detect_master` uses that flag to arbitrate between detectors, so a header fact was shadowed by a naming guess | `pixinsight.rs:58-61` decides `is_master` from `input.stack_count` when present; `pixinsight.rs:82` reports `input.stack_count.is_some()` |
| .2.25 | LIVE | **Yes.** `suggest` serves multi-kind requests, so a weak dark pair next to a strong bias reported `ambiguous`, and a near-tie across two kinds reported `match` | `ranking.rs:281` `multi_kind_suggest_status` groups by `CalibrationMatch::calibration_type` and reduces per kind; `suggest.rs:60` calls it. `batch.rs:113` already passed a per-kind subset and is unchanged |
| .13.28 | LIVE, weak | **Weak — no user-visible defect is claimed.** The predicate admitted the extensionless dotfile `.xisf` and the name `notxisf`. Reaching it needs such a file inside a watched project root, and the reconciler tolerates a spurious row | `watcher.rs:109` compares `Path::extension()`, ASCII-lowercased, against each entry with its leading `.` stripped. Both call sites (`reconciler.rs`, `src-tauri/src/watcher.rs`) are unchanged |
| .13.37 | LIVE as a duplicated rule | **No — no behaviour change.** The caller already filtered by tool, so no artifact was ever mis-attributed. The defect was that the documented owner of the rule did not enforce it, leaving a second caller free to omit it | `attribution.rs:66` takes `(artifact_id, artifact_tool, detected_at, current_launch_id)` and filters on tool; the caller's `.filter(|r| r.tool == new_launch_tool_id)` is deleted at `launches.rs:53-63` |

## Context

| File | Role |
|---|---|
| `crates/calibration/master-detect/src/lib.rs` | Modify: `parse_frame_type` tokenizes; `path_looks_like_master` splits segments; add `is_master_token` |
| `crates/calibration/master-detect/src/pixinsight.rs` | Modify: `stack_count` decides `is_master` and is reported as evidence; module doc corrected |
| `crates/calibration/core/src/ranking.rs` | Modify: extract `status_from_ranked_confidences`; add `multi_kind_suggest_status`; `suggest_status` unchanged for per-kind callers |
| `crates/app/calibration/src/matching/suggest.rs` | Modify: call `multi_kind_suggest_status` |
| `crates/workflow/artifacts/src/watcher.rs` | Modify: `extension_allowed` compares `Path::extension()` |
| `crates/workflow/artifacts/src/attribution.rs` | Modify: `reattribute_candidates` widens its tuple and owns the same-tool rule |
| `crates/app/lifecycle/src/artifact/launches.rs` | Modify: pass the producing tool per row; delete the caller-side tool filter |
| `crates/calibration/master-detect/tests/permutation_matrix.rs` | Modify: fixtures follow the token rule |

## Requirements

- `path_looks_like_master` splits on `/` and `\` unconditionally. No segment
  guard is gated behind `cfg(unix)` or `cfg(windows)`, and the `/`- and
  `\`-separated spelling of a fixture are both asserted on every platform.
- `parse_frame_type(FrameType::X.as_str()) == Some(FrameType::X)` for every
  variant the parser can produce.
- `extension_allowed` accepts a dotted and a dotless allowlist alike, and at
  least one case uses `DEFAULT_WATCH_EXTENSIONS`.
- `suggest_status` keeps its per-kind contract; `batch.rs` is not touched.
- `reattribute_candidates` is the only place the same-tool rule is written.

## Tasks

- [x] `path_looks_like_master` matches a delimiter-bounded `master`-prefixed
      token, keeping `_stacked`
- [x] `parse_frame_type` tokenizes on non-alphanumerics and matches whole tokens
- [x] `PixInsightDetector` reports and obeys a present `STACKCNT`/`NCOMBINE`
- [x] `multi_kind_suggest_status` reduces per kind; `suggest.rs` calls it
- [x] `extension_allowed` compares the real extension against a normalized
      allowlist
- [x] `reattribute_candidates` owns the same-tool rule; the caller-side filter
      is deleted
- [x] Module docs that stated the old behaviour are corrected
- [x] One regression case per finding, run red before the fix
- [x] Disclose every case that is green both ways rather than dropping it:
      `detect_master_prefers_stackcnt_evidence_over_naming` and
      `detect_master_prefers_stackcnt_negative_over_naming_positive` are vacuous
      for `.2.35` — `detect_master` (`lib.rs:111`) returns the first detector
      reporting `stack_count_evidence` and `SirilDetector` reports it whenever
      `IMAGETYP` parses, so Siril answered both and the PixInsight arm was
      unreached. They are kept as `detect_master` arbitration coverage and
      replaced for `.2.35` by
      `pixinsight::tests::a_present_stack_count_decides_this_detector_on_its_own`.
      `a_multi_kind_status_is_no_match_when_empty` (`ranking.rs:410`) is vacuous
      for `.2.25` and kept as boundary coverage only.
- [x] `cargo test` green for the five touched crates

## Done When

- [x] Every LIVE finding has a test that fails with the fix reverted
      (.2.30 2/2, .2.31 3/3, .2.35 1/1 direct-detector case plus 1/1 via
      `detect_master`, .2.25 2/3, .13.28 3/3 red; .13.37 proven by the inverted
      route)
- [x] Both separator spellings of the `.2.30` fixtures are asserted with no
      `cfg` gate
- [x] The five touched crates test green
- [ ] `infer_frame_type_from_path` no longer reads a frame word out of a target
      name — **deliberately not met here.** Out of scope by this unit's brief
      and filed as astro-plan-qs26q
