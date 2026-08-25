# Research: Inbox Mixed-Folder Split

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)
**Date**: 2026-05-22 (rewritten; prior draft 2026-05-20 superseded)

## Scope

Decide how to classify folder contents as `single_type`, `mixed`, or
`unclassified`; what evidence is authoritative; and how unclassified files
are surfaced and resolved. Output recommendations feed the classifier crate
boundary defined in plan.md.

**OVERRIDE NOTICE (2026-05-22)**: The prior draft of this file used
confidence scores, filename heuristics, and count-based thresholds to
classify folders. All of that model is **superseded**. Classification is now
**deterministic**: the sole authoritative signal is the FITS `IMAGETYP`
keyword, normalized via the `ImageTypNormalizationTable` below. There are no
confidence scores, no filename fallbacks, and no percentage thresholds.
(Ref: R-IMAGETYP, A5)

---

## Frame-Type Classification Model

### Primary and Sole Signal: FITS `IMAGETYP` Header

The classifier reads the `IMAGETYP` FITS keyword for every file in the
folder. Values are normalized via the `ImageTypNormalizationTable` (see
§IMAGETYP Normalization below). The classification is fully deterministic:

- **`single_type T`**: every file that has a readable `IMAGETYP` maps to the
  same `FrameType T`. Files with no readable `IMAGETYP` receive per-file
  unclassified markers but do not change the folder classification unless
  ALL files are unclassified.
- **`mixed`**: two or more distinct `FrameType` values appear among the
  classified files.
- **`unclassified`**: every file in the folder has no readable or recognized
  `IMAGETYP`.

There is no threshold. A folder with 1000 Light files and 1 Dark file is
`mixed`. A folder with 1000 Light files and 2 files with no `IMAGETYP` is
`single_type Light` with 2 per-file unclassified markers.

### Secondary: XISF Property Equivalents

XISF carries the same intent under `Observation:Type` or in
`FITSKeyword IMAGETYP` properties. PixInsight writes both. The adapter
crate `crates/metadata/xisf` SHALL surface the same `FrameType` enum from
either source.

### No Filename Heuristics

Filename heuristics are **not used** for classification. They are not
fallback evidence, do not reduce or raise any score, and have no effect on
the classification result. The sole classification signal is `IMAGETYP` (or
its XISF equivalent). (Ref: A5)

---

## IMAGETYP Normalization

**Section added 2026-05-22. (Ref: R-IMAGETYP-Norm)**

The normalization table is case-insensitive after trim. Unknown values cause
the file to be marked unclassified (per-file marker, not folder-level).

| FrameType  | Recognized IMAGETYP values |
|---|---|
| `Light`    | `LIGHT`, `Light Frame`, `Light`, `Object`, `LIGHT_FRAME`, `OBJECT`, `science` |
| `Dark`     | `DARK`, `Dark Frame`, `Dark`, `DARKFRAME` |
| `Bias`     | `BIAS`, `Bias Frame`, `Bias`, `BIASFRAME`, `Zero`, `OFFSET` |
| `Flat`     | `FLAT`, `Flat Frame`, `Flat`, `FLATFRAME`, `Skyflat`, `Domeflat` |

> **Note**: `dark_flat` IMAGETYP values (DARKFLAT, Dark Flat, FLATDARK, etc.) are intentionally NOT in the normalization table for v1. Per spec 007 R-DarkFlat-Reserved, dark-flat calibration matching is deferred. Files with these IMAGETYP values land in the `unclassified` state and require manual user assignment via `inbox.reclassify`. The `dark_flat` FrameType enum value remains reserved for forward-compat.

**Known capture-software values** (reference, not yet validated against real
captures):

- **NINA**: writes `Light Frame`, `Dark Frame`, `Flat Frame`, `Bias Frame`
- **Sequence Generator Pro (SGP)**: writes `Light Frame`, `Dark Frame`,
  `Flat Frame`, `Bias Frame`
- **Astro Photography Tool (APT)**: writes `LIGHT`, `DARK`, `FLAT`, `BIAS`
- **Voyager**: writes `Light Frame`, `Dark Frame`, `Flat Frame`, `Bias Frame`
- **Ekos/KStars**: writes `LIGHT`, `DARK`, `FLAT`, `BIAS`
- **MaximDL**: writes `Light Frame`, `Dark Frame`, `Flat Frame`, `Bias Frame`
- **ASIAIR**: writes `Light`, `Dark`, `Flat`, `Bias`
- **SharpCap**: writes `Light`, `Dark`, `Flat`, `Bias`
- **ZWO ASI software**: writes `LIGHT`, `DARK`, `FLAT`, `BIAS`
- **FireCapture** (planetary): primarily writes video — see §Video Lane

Validating these values against real FITS files from each capture software is
tracked in beads.

A settings UI for user-extended normalization mappings (for niche software)
is **deferred to v1.x**. This is documented as a spec 018 follow-up.
(Ref: R-IMAGETYP-Norm)

**Canonical data artifact**: the normalization table ships as data in
`crates/metadata/core` (not hardcoded in the classifier). (Ref:
R-IMAGETYP-Norm)

---

## Anti-Signal: `FILTER` — Not a Frame-Type Discriminator

`FILTER` is **not** a frame-type discriminator. Filter diversity within a
single frame type is a separate concern. Multi-filter folders with uniform
`IMAGETYP=Light` (e.g., LRGB or narrowband) are classified `single_type
Light`. The `{filter}` token in the spec 015 resolver routes files to
per-filter subdirectories at plan-generation time. (Ref: A6)

---

## Per-File Unclassified Markers

**Section added 2026-05-22. (Ref: R-FileMarker)**

When a file has no readable `IMAGETYP` (absent, malformed, or unknown
value), the file receives a per-file unclassified marker
(`InboxClassificationEvidence.unclassified = true`). This does not affect
the folder-level classification of the remaining files.

- A folder with 1000 Light files + 2 unclassified files → `single_type
  Light` with 2 per-file markers.
- A folder with 500 Light files + 500 Dark files → `mixed`.
- A folder where every file is unclassified → folder `unclassified`.

Per-file markers are surfaced in the UI as a "Needs review" sub-list. The
user resolves them via the inline reclassification affordance.

---

## Ambiguity Surfacing and Manual Reclassification

**Section rewritten 2026-05-22. (Ref: R-Unclass-1, R-Unclass-2)**

Files with `unclassified = true` (and no `manualOverride`) are surfaced in
the UI detail drawer as a "Needs review" sub-list. They are excluded from
folder-level consensus until resolved. A folder with only unclassified files
is blocked from confirmation.

### Resolution workflow

1. User opens the detail drawer for an `unclassified` or partially
   unclassified Inbox item.
2. The "Needs review" sub-list shows each unclassified file with an inline
   "Reclassify…" picker.
3. The file list supports multiselect (Shift+Click, Ctrl+Click, Select All)
   and a "Set type for selected" bulk action.
4. The `inbox.reclassify` contract (see `contracts/inbox.reclassify.json`)
   accepts a list of `{ filePath, frameType }` entries (single file or bulk).
5. The contract writes `manualOverride` to the corresponding
   `InboxClassificationEvidence` rows.
6. The classifier re-runs folder-level aggregation using overrides as
   authoritative (deterministic) evidence.
7. Once all files are either classified by `IMAGETYP` or overridden, the
   item transitions to `single_type` or `mixed` and the normal
   `inbox.confirm` CTA becomes available.

---

## Video Lane

**Section added 2026-05-22. (Ref: R-Video-1)**

Video files (`.ser`, `.avi`, `.mp4`, `.mov`) are detected at scan time and
routed to a **separate `inbox.video.*` lane** handled by
`crates/metadata/video/`. They:

- Do NOT enter the FITS classifier.
- Do NOT affect folder classification (a folder with FITS lights + 1 SER
  video file is classified on the FITS files alone; the video is handled
  separately).
- Are NOT assigned a `FrameType` from the FITS enum.
- Will be specified in detail in a future spec for planetary/lunar workflows.

The `lane` field on Inbox items: `enum("fits", "video")`.

---

## Recursive Scan and Inbox Item Granularity

**Section added 2026-05-22. (Ref: R-Granularity-1)**

The scanner walks the source root recursively. Each **leaf folder** that
directly contains FITS files becomes its own Inbox item. Folders that
contain only subfolders (no direct FITS files) are not Inbox items; their
FITS-bearing descendant folders are.

`InboxItem.relativePath` is the path of the leaf folder containing the FITS
files.

---

## Split Destination Model

**Section added 2026-05-22. (Ref: R-Split-1)**

Split plans produce destination paths via the spec 015 resolver, targeting
**Inventory paths directly**. There is no Inbox sibling staging step. Each
`inbox.confirm` action produces one plan. Plan items carry final Inventory
destinations resolved at confirm time.

---

## Content Signature and TOCTOU Safety

**Section added 2026-05-22. (Ref: A8, R-Sig-1)**

### Signature Formula

- **Per-file signature**: `sha256(filename || size_bytes || mtime_unix_ns || sha256(first 65536 bytes))`
  - The 64 KB partial-content hash detects FITS header rewrites that
    preserve size and mtime.
- **Folder content signature**: `sha256(sorted(per_file_signatures))`

### Usage

- `inbox.classify` computes and returns the folder `contentSignature`.
- `inbox.confirm` requires the caller to supply the `contentSignature` from
  the most recent classify call.
- If the signature does not match the current folder state, the operation
  returns `classification.stale` with a `staleSince` timestamp and the
  caller must re-classify.
- **Cost**: one `stat` call + 64 KB read per file. Acceptable for typical
  inbox folder sizes (< 10 k files).

---

## `plan_open` State and Repair Query

**Section added 2026-05-22. (Ref: R-PlanOpen)**

`plan_open` is a stored `InboxItem.state` value. The primary update path is
via the spec 002 event bus (`plan.applying.completed`, `plan.applying.paused`,
`plan.discarded`). A background self-healing repair query runs every 5
minutes to close any `plan_open` items whose linked plan has reached a
terminal state (`applied | partially_applied | failed | cancelled |
discarded`) without the event having been processed.

---

## Prior Art

### PixInsight WBPP File Classifier

PixInsight's WeightedBatchPreprocessing script classifies inputs by reading
FITS headers with a deterministic priority list (`IMAGETYP` → `FRAME` →
user override → filename). Astro Plan's classifier mirrors WBPP's
header-first model and stops at frame type. (Ref: A5)

**Takeaway**: header-first is the established expert pattern. We adopt it
and remove the filename fallback entirely.

---

## Container Differences (FITS vs XISF)

XISF supports nested properties and multiple frame types per file (rare).
The classifier MUST flatten to one `FrameType` per file. Files with multiple
frame-type properties are treated as unclassified with high-priority
surfacing in the UI.

---

## Performance

- Header reads: a single FITS header read is a few KB at the file head.
  Reading 500 headers is I/O-bound; sequential reads of ~1 MB total are
  comfortably under the 2 s target on SSD.
- Caching: classification results are cached in `inbox_classifications`
  keyed by (inbox_item_id, content_signature). Invalidated on
  `force_rescan` or content_signature drift.
- Content signature: one stat + 64 KB read per file. Per constitution,
  full-file hashing is not used for classification.

---

## Resolved Open Questions

All open questions from the prior draft are resolved (2026-05-22):

1. ~~Confidence thresholds~~ — replaced by deterministic IMAGETYP model. (Ref: R-IMAGETYP, A5)
2. ~~Filter mismatch~~ — not a mixed condition; `{filter}` routes at plan time. (Ref: A6)
3. ~~Split destination model~~ — direct to Inventory. (Ref: R-Split-1)
4. ~~Manual reclassification~~ — inline picker + multiselect bulk-assign. (Ref: R-Unclass-1, R-Unclass-2)
5. ~~Video files~~ — separate lane. (Ref: R-Video-1)
