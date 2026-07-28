# Feature Specification: Inbox — Drop Parent Items

**Feature Branch**: `spec/058-inbox-drop-parent-items`

**Created**: 2026-07-19

**Status**: Implemented (post-hoc record, verified 2026-07-28). Placeholder
creation is removed, `inbox_classify_source_group` ships, source groups render
with a Classify action, `"mixed"` is retired from the classifier, and the
migration citations are corrected. The unchecked and `BLOCKED` annotations in
`tasks.md` describe a state HEAD has since resolved; that file is legacy paper
and task state lives in beads. Outstanding verification gates (`T030`, `T042`,
`T048`) and the e2e helper split (`T014`-`T016`) are tracked as beads.

**Input**: Product-owner decision: "Drop the concept of parent inbox items
entirely. A folder produces one or more real inbox items, never a parent plus
children. Homogeneous folder → exactly one item. Mixed folder → N sibling
items. Siblings are linked by sharing a source group. Upon split
classification, the linkage is broken. Greenfield — no backwards
compatibility."

**Resolves**: [#711](https://github.com/platevault/platevault/issues/711)
**Instance A, unsplit remainder only** — a list row reporting `CLASSIFIED` for
an item that is unclassified, in the case of a folder that does *not* split.

The scope claim is deliberately narrow, because ground is already held:

- **The split case is fixed on `main`.** PR #1038 (`1eae04e9`) hid the aggregate
  row whenever any sibling existed; PR #1081 (`b4e72263`) narrowed that to
  genuine splits, which repaired #1038's regression. Between them, a folder that
  splits no longer shows a lying aggregate row.
- **Instance B is closed.** The false-`NEEDS REVIEW` direction was closed by
  PR #1086 (`22f94a9e`); see
  [Relationship to #711 Instance B](#relationship-to-711-instance-b).

- **Instance A's visible badge is fixed on `main`.** PR #1099 (`ef90b074`)
  landed after this specification was written, making the list badge read the
  item's own classification result rather than falling back to `state`.

  📎 **#1099 alone was not sufficient — #1206 finished it.** Two accounts of
  this looked contradictory and are in fact both correct, about different
  commits. The #1208 bisect found
  `inbox_ui_unsplit_unclassified_folder_badge_is_not_classified` failing on
  `ef90b074` — the very commit credited with the fix — **and** on `6fa1bf55`,
  across the ubuntu 2/4 and windows 2/2 shards. Both refs **predate #1206**,
  which repaired the journey's stale selector (following the gap #1202
  identified). #1099 fixed the badge *expression*; the journey went on failing
  on a selector until #1206.

  Confirmed green on this branch 2026-07-20: the full Real-UI E2E suite passed
  on head `33f78450`, **all six shards, including the ubuntu 2/4 and windows
  2/2 that the bisect saw fail**. Note that pre-#1268 `main` could not have
  settled this either way — its E2E runs were cancelled by a concurrency-group
  bug before reporting, so green-on-`main` in that window meant silence, not a
  pass. The scope claim below is stated against SC-003 regardless, which is
  measurable independently of this journey.

What remains — and what this feature is actually for — is the folder that
produces exactly one item, and specifically **the false row underneath it**.
#1081's narrowing knowingly returned Instance A for that case, because there is
no sibling whose existence could suppress the placeholder; #1099 then stopped
the badge repeating the row's claim. Neither removed the row. This feature
removes the placeholder itself.

**Read the scope claim as SC-003, not as the badge.** After #1099 the
user-visible disagreement is gone, so a reader checking this feature against a
screenshot will find nothing wrong. The defect that remains is in the database:
a row with `state = 'classified'` and no frame type, which confirm can still
bind a filesystem plan to. That is what SC-003 measures and what this feature
exists to remove.

**The visible unsplit-case symptom was subsequently patched a third time by
PR #1099** (merged 2026-07-20, after this scope claim was written): the list now
reads `classification_result` instead of `state` for the badge, so #711
Instance A no longer reproduces on `main`. This does not narrow this feature's
scope — see [Product Intent](#product-intent) for why a third read-side patch
is evidence for the model-level fix, not a reason to drop it. `upsert_inbox_sub_item`
still writes the hardcoded `state = 'classified'` literal #1099 read around.

**Supersedes the read-side workarounds in**: PR #1038, PR #1081, and PR #1099.
Those workarounds are removed (FR-026, SC-007) rather than corrected — with no
aggregate row there is nothing to suppress.

## Product Intent

A user who points PlateVault at a folder of subframes expects the Inbox to show
them **what is in that folder**. If the folder holds one kind of frame, that is
one thing. If it holds lights and darks together, that is two things. In both
cases every row the user sees should be a row they can act on: select it, read
its frame type, confirm it, and find its plan afterwards.

Today the Inbox shows something else. Every scanned folder produces an extra,
invisible-by-intent bookkeeping row — a "folder placeholder" — that carries no
frame type and no classification, yet is the row the user actually selects and
confirms for ordinary folders. For a folder that splits, the placeholder is
superseded by the real single-type rows but is not removed, so it lingers in the
list advertising a classification it does not have.

This produced a visible, reproducible lie: in #711 Instance A a list row read
**CLASSIFIED** while opening it showed `unclassified`, a blocking
"Frame types required" banner, and a disabled Confirm button — the list and the
detail panel disagreeing about the same item id.

**That visible symptom is now closed.** PR #1099 (merged 2026-07-20T03:22:26Z)
changed the list to read the item's own cached `classification_result` instead
of falling back to `state` for the Type-column badge, so the two surfaces agree
on screen again. `inbox_ui_unsplit_unclassified_folder_badge_is_not_classified`
(`crates/e2e-tests/tests/inbox_ui_journeys.rs:562`) is the acceptance test for
exactly this defect; once its stale selector was repaired (#1206, following the
gap #1202 identified), it passes on `main`.

The badge no longer lies, but **the database still does**. `upsert_inbox_sub_item`
(`crates/persistence/db/src/repositories/inbox.rs:525` INSERT `VALUES`, `:532`
`ON CONFLICT ... DO UPDATE SET`) hardcodes `state` to the SQL literal
`'classified'` regardless of `frame_type`, so a placeholder that has never been
classified is persisted as `classified` with a null frame type for as long as it
stays unresolved. That is SC-003's failure condition — observable database
state, not a transient rendering window. #1099 changed how the list reads
around that row; it did not change what the row says.

This is the **third** time the read side has needed patching for the same
underlying lie: #1038 and #1081 patched it before, and #1099 is a third — one
that landed while this feature was already being planned. That is not evidence
against this feature; it is the strongest evidence for it. The read side keeps
needing patches because the model, not the query, is wrong. This feature removes
the parent concept so that every row in the Inbox is a real, actionable item
that states only true things about itself, closing the defect at its source
rather than at its latest presentation. (Refs #711, #1099, #1202, #1206.)

## Recorded Decisions

These were decided by the product owner before specification. They are recorded
here so a future reader can see they were considered, not overlooked. They are
not open for re-litigation during planning.

### D-001 — A folder produces only real items

A classified folder yields **exactly one** inbox item when all its files belong
to one group, and **N sibling items** when classification finds N distinct
groups. There is no parent row, no placeholder row, and no aggregate inbox item
in any case.

D-006 completes this: before classification the folder has *no* inbox items at
all, so there is never a moment at which an aggregate item exists.

### D-002 — Sibling linkage is a set relationship, not pairwise

Siblings are related by sharing one source group. This must hold for any N, not
only N = 2. Nothing in the model may assume a folder splits into at most two
items, and nothing may designate one sibling as primary, first, or authoritative
on behalf of the others.

### D-003 — Split classification breaks the linkage

Once classification has split a folder into distinct single-type items, those
items are **independent**. See [Linkage Semantics](#linkage-semantics) for the
exact meaning — this is a required deliverable of the spec, not an aside.

### D-004 — Greenfield: no backwards compatibility

**Decided by the product owner. This is a decision, not an oversight.**

No migration is written for existing parent+child rows. Legacy `sg-migrate-*`
source-group behaviour is not preserved. The model is not designed around
existing `plan_open` parent rows surviving the change. Existing library
databases do not need their inbox state preserved.

**Accepted risk**: under this change, an existing database's **open inbox plans
and confirmed-but-unapplied inbox items would be stranded** — the rows the plans
and confirmations bind to are the very rows being removed.

**Why that risk is acceptable right now**: there are no current installs of the
app (product owner, resolving Q-1; **re-confirmed 2026-07-20** after the
condition below was tested). The stranding cannot reach a user because no user
holds a library database. No migration UX, first-launch reset notice, or
legacy-row detection is designed.

*Evidence gathered 2026-07-20, because releases now exist and the literal
phrase "no current installs" needed re-testing rather than re-assertion: five
public releases (v0.1.0–v0.5.0) carry roughly **four installer downloads in
total** across all of them — 1 `.exe` on v0.2.0, 3 `.dmg` on v0.5.0, every other
artifact at zero. The large `latest.json` counts are auto-updater polling, not
installs. The repository has 0 stars and one fork (`TFGSUMIT/alm`, created
2026-07-14, no commits pushed to it) — a source-level fork is not an install and
does not carry a library database. Owner confirmed: no current installs.*

**This licence is conditional on that fact.** If any part of this work lands
after the product has real users, the question reopens — see
[PENDING_REVIEW_QUESTIONS.md](PENDING_REVIEW_QUESTIONS.md) Q-1. It is answered
for the current moment, not in principle.

### D-005 — A superseded sibling is invalidated, not preserved

When a re-scan no longer produces a sibling that has an open plan, the system
**invalidates** it: the item is marked superseded, its plan is blocked from
application pending the user's decision, and an explicit signal tells the user
their confirmation was superseded.

The confirmation was made against a world that no longer exists, and a plan
describes pending filesystem mutations, so silently honouring it is the
dangerous option (Constitution II). Rejected: keep-and-show (a row for a group
that no longer exists), keep-and-hide (an invisible open plan — the worst
failure mode), and block-re-scan-until-resolved (one stale plan freezes
reconciliation for the whole folder, denying the user the obvious remedy).

**Refined by the Q-6 decision**: *supersede and surface, never silently cancel.*
The orphaned sibling is marked superseded and the user decides what happens to
its plan. Automatic cancellation was the original reading of "invalidate", but a
plan is a record of pending filesystem mutations, and disposing of one without
the user's involvement is the same class of act Constitution II exists to
prevent. The decision's substance — a superseded sibling is not preserved as
though nothing happened — is unchanged.

**The mechanism is descoped from this feature** and delivered by the follow-on
micro-spec, together with FR-020/021/022. The staleness guard Q-5 answers is
per-item and stays per-item; it is not repurposed as the invalidation hook.

### D-006 — Scan creates the source group; classification creates the items

Scan creates **only** the source group. The Inbox list shows unclassified
source groups alongside classified items, so a freshly scanned folder is
visible immediately. Once classification runs, that source-group row is
replaced by its N item rows.

This is what makes D-001 hold *unconditionally* rather than eventually: no
`inbox_items` row representing a whole folder is ever created, not even
transiently.

Rejected: **scan creates one provisional item that classify splits** — a
transient parent is still a parent, and it revives the selection-churn coupling
that caused the #1038 outage (FR-023). Rejected as primary but **retained as a
fallback**: **scan creates the source group and nothing is displayed until
classify** — because classify is a per-item command rather than automatic on
scan, the invisible window is not momentary; a user could scan a whole drive and
be shown an empty Inbox.

**Known cost, accepted**: the Inbox list becomes a union of two row types, and
selection must survive the "one source-group row becomes N item rows"
transition. This is real design work for the plan gate, not a solved problem.

### D-007 — Siblings are made legible by grouping on the folder

The Inbox list gains a **folder grouping dimension**, keyed on the source group
that siblings already share. Multiple rows per folder become legible as "one
folder, N frame types" rather than N unexplained near-identical rows.

This reuses the existing grouping machinery rather than adding a bespoke
sibling affordance. Whether folder becomes the Inbox *default* grouping, and the
one engine limitation this runs into, are recorded in Q-8.

## Linkage Semantics *(required by D-003)*

"The linkage is broken" is made precise as a split between two kinds of
relationship. The source group survives as an **identity** relationship and is
extinguished as a **lifecycle** relationship.

### What the source group still means after a split

The source group continues to answer exactly one question: *which inbox items
were derived from this folder?* It is retained because the folder on disk is
still one folder — a re-scan or re-classification reads one directory and must
reconcile its result against the rows that directory previously produced. It is
the anchor for folder-level facts that are genuinely folder-level: the root, the
relative path, the folder content signature, the format, and the lane.

### What the source group no longer means after a split

It confers no shared lifecycle. Concretely, after a folder has split into
siblings:

- **No shared state.** Each sibling has its own state. Confirming one sibling
  does not move another out of the queue.
- **No shared classification.** Each sibling carries its own frame type and its
  own classification result. No sibling's badge is derived from another's.
- **No shared plan.** Each sibling binds its own plan independently. A plan
  opened against one sibling neither blocks nor represents the others.
- **No shared signature.** Each sibling has its own content signature covering
  only its own files, and its own staleness verdict at confirm time.
- **No delegation.** No operation resolves "the item for this folder" and acts
  on it. Any code path that needs to act on a folder acts on an explicit,
  named item, or on all siblings explicitly.

### Observable difference

Before: selecting a split folder's placeholder row shows a state badge derived
from a row that has no frame type, and confirming it operates on a row whose
evidence spans multiple frame types.

After: there is no such row to select. Every selectable row has a frame type,
a truthful badge, and a confirm outcome that concerns only its own files.

### The one operation that stays folder-wide

Re-classification necessarily re-reads every file in the directory, because
frame type is derived from file headers and the files share a directory. It is
therefore scoped to the source group by construction. This is **re-derivation,
not lifecycle coupling**: it recomputes what the folder's items should be and
reconciles the existing rows against that answer. It does not propagate state,
plans, or confirmations between siblings.

The reconciliation rules this implies — in particular what happens to a sibling
that a re-scan no longer produces but that already has a plan — are answered in
principle by D-005 (invalidate, do not preserve). Their *mechanism* is
deliberately not built here; see below.

### The one lifecycle coupling that knowingly survives

This feature declares shared lifecycle dead, and then ships one exception. It is
recorded here rather than left to be discovered.

`reclassify_v2` (`crates/app/inbox/src/reclassify.rs:347-360`) refuses to
re-derive a folder when **any** sibling in that source group has an open plan.
That is a folder-wide block, and it contradicts "no shared plan" above. It
survives 058 untouched.

It survives because it is load-bearing safety, not an oversight. Re-derivation
ends by purging groups that no longer exist, via `delete_sub_item_if_unlinked`
(`crates/persistence/db/src/repositories/inbox.rs:610-619`), which **refuses to
delete a plan-linked row and does so silently**. Removing the interlock without
first building an invalidation path would therefore not invalidate the orphaned
sibling — it would leave it in the queue with an open plan and nothing on disk
behind it. That is precisely the "keep and show" outcome D-005 rejects. The
interlock is what prevents the system reaching that state today.

Per-item `reclassify` (`:82`) blocks only on the item's own plan. That narrower
check is already consistent with D-003 and is not in question.

**Consequence for the reader**: until the follow-on lands, a folder with one
confirmed sibling cannot have its other siblings reclassified. This is real,
user-visible friction, accepted knowingly for one release rather than resolved
by rushing a plan-lifecycle design.

### Target architecture for retiring the coupling *(owner-approved)*

Recorded so the follow-on does not re-derive it:

- **Per-item reclassify becomes the only user-facing classification action**,
  blocking on that item's own plan and nothing else.
- **Folder re-derivation becomes a separate identity-scoped operation**,
  triggered by disk change rather than by a user gesture, that never blocks.
- **The interlock is retired by irrelevance, not deleted defensively.** Once no
  user-facing action performs folder-wide re-derivation, the folder-wide block
  has nothing left to guard and can be removed as dead code — not removed first
  and compensated for afterwards.
- **Supersede and surface, never silently cancel.** An orphaned plan-linked
  sibling is marked superseded and shown to the user, who decides. The system
  does not discard a plan on the user's behalf (Constitution II).
- **Delivered by event-driven inversion**, following the existing
  `crates/app/inbox/src/plan_listener.rs` pattern — **not** by adding a
  `crates/app/core` dependency edge to `crates/app/inbox`. `cancel_plan` lives
  at `crates/app/core/src/plan_apply.rs:1871` and `crates/app/core` is not a
  dependency of `crates/app/inbox`; inverting the call preserves that boundary.
  `plan_listener.rs:138-140` already returns an item to `classified` when a plan
  reaches `partially_applied`, `failed`, or `cancelled`, so this is an extension
  of an established pattern rather than new territory.

See [PENDING_REVIEW_QUESTIONS.md](PENDING_REVIEW_QUESTIONS.md) Q-6.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Every Inbox row tells the truth about itself (Priority: P1)

A user scans a library root and reviews the Inbox queue. Every row shows a frame
type and a state that match what they see when they open that row. There is no
row whose badge contradicts its own detail panel, and there is no row that
cannot be acted upon.

**Why this priority**: This is #711, the defect that motivates the feature, and
it is a trust defect. A user who catches the Inbox stating something false about
their library has no basis for trusting the destructive plans it proposes later.
Everything else in this feature is machinery in service of this outcome.

**Independent Test**: Scan a folder of uniform lights and a folder mixing lights
and darks. For every row in the resulting queue, compare the list badge with the
detail panel and with the item's own classification result. All three agree for
every row, in both folders.

**Acceptance Scenarios**:

1. **Given** a folder whose files are all light frames, **When** the user scans
   and classifies it, **Then** the Inbox shows exactly one row for that folder,
   badged `light`, and opening it shows a detail panel that agrees.
2. **Given** a folder containing both lights and darks, **When** the user scans
   and classifies it, **Then** the Inbox shows exactly two rows for that folder
   — one `light`, one `dark` — and no third row.
3. **Given** any row in the Inbox queue, **When** the user selects it, **Then**
   the detail panel's classification badge matches the list row's badge.
4. **Given** any row in the Inbox queue, **When** the user reads its state
   badge, **Then** that state reflects a classification the row actually
   carries — no row reports `classified` without a frame type.

---

### User Story 2 — Confirming an ordinary folder still works end to end (Priority: P1)

A user scans an ordinary single-type folder, confirms it to inventory, and then
reviews and applies the resulting plan. The row they selected stays selected
through classification and confirmation, and "Review plans" appears.

**Why this priority**: Equal to US1 because this is precisely the regression
#1038 caused and #1081 repaired. The parent row is today the row that confirm
and plan links bind to for a homogeneous folder; removing it without moving
those bindings would reintroduce the exact failure that blocked roughly nine
PRs. This story exists to make that failure mode a first-class acceptance
criterion rather than something discovered in E2E.

**Independent Test**: Scan a uniform folder, confirm it, and assert the plan is
reachable on the plan surface and applies to the shown destination — the three
Real-UI journeys that #1038 broke.

**Acceptance Scenarios**:

1. **Given** a scanned uniform folder, **When** the user confirms it, **Then**
   a plan is created bound to that row and "Review plans" becomes available.
2. **Given** a confirmed uniform folder with an open plan, **When** the user
   opens the plan surface, **Then** that folder's plan is listed.
3. **Given** a user has selected a row, **When** classification completes,
   **Then** the selection is not silently dropped.
4. **Given** a user bulk-reclassifies frame types on a blocked folder, **When**
   reclassification completes, **Then** Confirm becomes enabled.

---

### User Story 3 — A mixed folder's parts are handled independently (Priority: P2)

A user scans a folder holding lights, darks, and flats. They get three rows.
They confirm the lights now and leave the darks and flats for later. The lights
leave the queue; the darks and flats stay, unaffected.

**Why this priority**: This is the payoff of D-003 and the reason Option 1
(parent always, never split) was rejected. It is P2 rather than P1 because the
mixed case is less common than the uniform case, but it is the case that
justifies the whole model.

**Independent Test**: Scan a three-type folder, confirm exactly one sibling, and
assert the other two are untouched in state, classification, and plan binding.

**Acceptance Scenarios**:

1. **Given** a folder that split into three siblings, **When** the user confirms
   one, **Then** the other two remain in the queue with their own states.
2. **Given** a folder that split into N siblings, **When** the user opens plans
   after confirming two of them, **Then** exactly two plans exist for that
   folder, each naming its own frame type.
3. **Given** a folder that split into N siblings, **When** the user views the
   Inbox summary counts, **Then** the folder's contribution to those counts
   matches the number of rows shown in the list.

---

### User Story 4 — Machine-derived classification is not re-asked (Priority: P2)

When a mixed folder splits, the user is not asked to re-state the frame type of
each resulting item. The split items arrive already classified, because the
frame types were derived from the files' own headers.

**Why this priority**: This is the reason Option 2a was rejected. Re-asking the
user to classify a split's output re-does machine work on a known answer and
turns a helpful split into busywork proportional to N.

**Independent Test**: Scan a mixed folder with readable headers and assert every
resulting sibling carries a frame type without any user input.

**Acceptance Scenarios**:

1. **Given** a mixed folder whose files carry readable frame-type headers,
   **When** it splits, **Then** each resulting sibling is already classified and
   requires no further user classification.
2. **Given** a folder where some files have unreadable frame types, **When** it
   splits, **Then** only the unclassifiable files are gathered into a
   needs-review item, and the classifiable siblings are unaffected by it.

---

### Edge Cases

- **A folder with no classifiable files at all.** Every file is
  unclassifiable. The folder must still produce a row the user can see and act
  on, rather than vanishing — it becomes a single needs-review item, not zero
  items and not a placeholder.
- **A needs-review item that the user fully resolves.** Once the user assigns
  frame types such that nothing is left needing review, the item must stop
  reporting needs-review. This currently relies on an in-place group-key rewrite
  to a synthetic value chosen specifically to dodge a uniqueness constraint
  against a sibling — see [research.md](research.md) §4.
- **A re-scan that changes the split.** A folder previously split into three
  now yields two, or one, or four. Rows that still correspond to a group
  survive; a row that no longer corresponds to any group is invalidated, and if
  it carried an open plan it is marked superseded and surfaced to the user, who
  decides the plan's fate (D-005). *The mechanism ships in the follow-on
  micro-spec, not here.*
- **A re-scan that finds the folder unchanged.** Must not churn item ids, or
  the user's selection and any open plans are disturbed for no reason.
- **Two folders at the same relative path under different roots.** Identity must
  remain root-scoped — including when grouping the list by folder, which is why
  D-007 groups on the source group rather than on the path string.
- **A folder scanned but never classified.** Stays visible as its source-group
  row indefinitely; it must not silently disappear from the queue, and it must
  not be confirmable (there is nothing yet to confirm).
- **A folder whose files all move into one group after reclassification.** A
  previously split folder becoming homogeneous must converge on a single item
  without leaving orphans.
- **A needs-review item whose files receive two *different* user-supplied frame
  types.** This state remains reachable after 058 and reports `mixed`. The
  `mixed` affordance is therefore retained — see Q-9. **Open design gap for the
  plan gate**: resolving a heterogeneous needs-review bucket into two frame types
  is the *expected* outcome of the user doing exactly what they were asked to do,
  and this feature's own model says two types means two siblings. So the item
  arguably ought to **split**, rather than come to rest in `mixed` with Confirm
  disabled — which is a dead end the user can only escape by re-editing answers
  that were correct. This is recorded as a gap to size, not a decision taken; it
  inherits the item-identity/remount hazard already documented at
  `crates/e2e-tests/tests/inbox_ui_journeys.rs:390-399`, because splitting moves
  the resolved files onto a different item id mid-interaction.

## Requirements *(mandatory)*

### Functional Requirements

**Model**

- **FR-001**: The system MUST NOT create any inbox item that lacks a
  classification identity, at any point in the folder's lifecycle. *Read with
  the FR-015 scoping note: a detected calibration master carries its own frame
  type, filter and exposure read from the file, so it has a classification
  identity and is not an exception to this requirement. Its empty stored
  `group_key` is a storage artifact, not an absence of identity — see
  [#1157](https://github.com/platevault/platevault/issues/1157), which requires
  placeholder-scoped predicates to stop treating that empty value as a
  discriminator.*
- **FR-002**: A classified folder whose files all belong to one group MUST
  produce exactly one inbox item.
- **FR-003**: A classified folder whose files belong to N distinct groups MUST
  produce exactly N inbox items.
- **FR-004**: The system MUST NOT create, retain, or expose an inbox item that
  represents a folder as a whole in addition to that folder's real items.
- **FR-005**: Items derived from one folder MUST be identifiable as a set via
  their shared source group, for any N.
- **FR-006**: The system MUST NOT designate any sibling as primary or
  authoritative on behalf of its siblings.

**Truthfulness**

- **FR-007**: Every inbox item MUST carry a classification identity that is
  consistent with its own reported state; the system MUST NOT record a state of
  `classified` on an item that carries no frame type.
- **FR-008**: The classification shown for an item in the Inbox list MUST agree
  with the classification shown in its detail panel and with the item's own
  classification result, for every item.
- **FR-009**: Inbox summary counts MUST count each visible item consistently
  with the list, with no folder counted twice and none omitted.
- **FR-028** *(Q-7)*: Needs-review MUST be represented as its own field, distinct
  from the item's classification identity. One field MUST NOT simultaneously
  carry classification identity, needs-review status, and a uniqueness
  discriminator.
- **FR-029** *(Q-7)*: Resolving a needs-review item MUST record its frame type,
  its classification identity, and its `classified` state together, as one
  atomic transition. No intermediate state may be observable in which the item
  reports `classified` without a frame type.
- **FR-030** *(Q-7)*: Frame-type agreement across an item's files MUST NOT by
  itself cause that item to be reported classified; the mandatory-attribute gate
  MUST pass, and the API response, the item row, and the classification cache
  MUST agree on the result.
- **FR-031** *(Q-9)*: The `mixed` classification result and its user-facing
  affordance MUST be retained for as long as placeholder rows exist. It remains
  reachable on the **pre-materialization placeholder** of a folder whose files
  span two or more frame types, which reports API `mixed` while its row stores
  `unclassified`. It is *not* reachable on a needs-review item carrying
  conflicting user-supplied frame types — that route was recorded originally
  and refuted at Layer 2, because the re-materialization rebuild clears
  `manual_override` from the evidence rows. Since this feature removes
  placeholder rows, **this feature is itself what makes `mixed` unreachable**.
  *The plan gate has decided: PG-1 retires `mixed` rather than re-scoping it,
  because the affordance attaches to the placeholder this feature deletes, so
  retiring it is dead-code removal. Implemented by T035, which MUST also replace
  the `inbox-mixed-alert` sync signal in the same change — see `tasks.md`
  sequencing constraint 4.*

**Lifecycle**

- **FR-010**: Confirmation MUST operate on exactly one inbox item and MUST NOT
  alter the state, classification, or plan binding of that item's siblings.
- **FR-011**: A plan MUST bind to exactly one inbox item.
- **FR-012**: Each sibling MUST be independently confirmable while its siblings
  remain unconfirmed.
- **FR-013**: Staleness detection at confirm time MUST be evaluated against the
  files belonging to the item being confirmed. *Confirmed unchanged by the Q-5
  decision: staleness is a per-item property. Reclassification MUST compute a
  real per-group signature rather than the empty-set hash constant. **Delivered
  by #1105** (`038781e2`): `root_absolute_path` is a field on
  `InboxReclassifyV2Request` and `reclassify.rs:827` joins it per file. No work
  remains under this requirement.*
- **FR-014**: Re-classification MUST re-derive a folder's items from the files
  on disk without propagating state, plans, or confirmations between siblings.

**Scan and classification boundary** *(D-006)*

- **FR-015**: Scanning a folder MUST create its source group and MUST NOT
  create an inbox item representing the folder as a whole. *Self-describing
  file-level items are unaffected: a detected calibration master (spec 040)
  carries its frame type, filter and exposure from the file itself, so its row
  asserts nothing that classification has yet to determine. `persist_master_item`
  already creates one row per master file, and a folder containing only masters
  already produces no aggregate row at all
  (`apps/desktop/src-tauri/src/commands/inbox.rs:356`) — that is this decision's
  target shape, already shipped.*
- **FR-016**: A scanned but unclassified folder MUST be visible in the Inbox
  list, represented by its source group.
- **FR-017**: When classification completes, the folder's source-group row MUST
  be replaced in the list by that folder's item rows.

**Re-scan and invalidation**

- **FR-018**: Re-scanning an unchanged folder MUST NOT change the identity of
  its existing items.
- **FR-019**: The system MUST anchor folder-level re-scan comparison to the
  source group rather than to any single item.

*FR-020, FR-021 and FR-022 have been moved out of this feature.* They covered
plan invalidation on supersession, the user-facing supersession signal, and the
removal of the folder-wide reclassify block. They are delivered by a follow-on
micro-spec — see
[The one lifecycle coupling that knowingly survives](#the-one-lifecycle-coupling-that-knowingly-survives)
for why, and for the owner-approved target architecture the follow-on
implements. D-005 remains a recorded decision of this specification; only its
mechanism is descoped.

See `specs/tiny/reclassify-split-per-item-and-rederivation.md` (PR #1097).

The numbers are retired rather than reused, so that a reader of PR history or of
the follow-on is never left wondering whether FR-020 means one thing here and
another there.

**User-visible continuity**

- **FR-023**: A user's selection MUST NOT be silently dropped as a result of
  classification replacing a source-group row with item rows.
- **FR-024**: After confirming an item, the resulting plan MUST be reachable on
  the plan surface.
- **FR-025**: The Inbox list MUST offer grouping by folder, so that items
  derived from one folder can be viewed together (D-007).

**Removal**

- **FR-026**: The system MUST NOT retain read-side logic whose only purpose is
  to hide a superseded aggregate row from lists or counts.
- **FR-027**: No migration of existing parent-and-child inbox rows is provided
  (D-004).

### Key Entities

- **Source group**: The folder-level identity for a scanned directory — its
  root, its path relative to that root, its content signature, its format, and
  its lane. Owns facts that are true of the directory itself. Relates to zero or
  more inbox items: zero after scan, N after classification. **Is not an inbox
  item**, but is shown as a queue row while the folder is unclassified (D-006),
  and is the key the folder grouping dimension groups on (D-007).
- **Inbox item**: One actionable unit of the queue — a set of files from one
  folder that share a classification identity. Carries its own frame type,
  state, evidence, signature, and at most one plan binding. Every item is
  selectable, and every item states only facts about its own files.
- **Sibling set**: The inbox items derived from one source group. A set, of any
  size, with no distinguished member and no shared lifecycle.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For every item in a scanned queue, the list badge, the detail
  panel badge, and the item's own classification result agree — zero
  disagreements, measured across uniform, mixed, and needs-review folders.
  (This is the #711 exit condition.)
- **SC-002**: Classifying a folder with one frame type yields exactly one queue
  row; classifying a folder with N frame types yields exactly N rows.
- **SC-002b**: A scanned but unclassified folder is visible in the Inbox as
  exactly one row, and that row is not an inbox item (D-006).
- **SC-003**: Zero inbox items exist in any state carrying a `classified` state
  without a frame type.
- **SC-004**: The Inbox summary counts equal the number of rows the list shows,
  for every combination of uniform, split, and needs-review folders.
- **SC-005**: The three Real-UI journeys that regressed under #1038 pass:
  catalogue-in-place zero-moves, confirm-then-apply-to-shown-destination, and
  bulk-reclassify-unblocks-confirm.
- **SC-006**: Confirming one sibling of an N-way split leaves the other N−1
  unchanged in state, classification, and plan binding.
- **SC-007**: Every read-side predicate that exists solely to suppress an
  aggregate row is deleted, and no replacement suppression logic is introduced.
- **SC-008**: Re-scanning an unchanged folder produces no item identity churn.
- **SC-009** *(**NOT met by this feature — do not tick.** D-005 remains a
  recorded decision, but its mechanism is descoped to
  `specs/tiny/reclassify-split-per-item-and-rederivation.md` together with the
  retired FR-020/021/022. T043 records this. **The real exit bar for 058 is the
  other eleven criteria.**)*: When re-derivation removes an item that had an
  open plan, that item is marked superseded, its plan is blocked from
  application pending the user's decision, and the user receives an explicit
  superseded signal — zero cases of a silently discarded or silently retained
  plan (D-005).
- **SC-010**: A user can group the Inbox by folder and see each folder's
  siblings together under one header (D-007).
- **SC-011**: #711 Instance B stays fixed — reclassify does not report
  `classified` while mandatory attributes are still missing (the `22f94a9e`
  regression test continues to pass).

## Rejected Alternatives

Recorded with tradeoffs so they are not revisited without new information.

### Option 1 — Keep the parent always; never split

Make the folder-level row the single authoritative item in all cases, and never
materialize single-type items at all. A mixed folder stays one row.

**Genuine advantage**: The simplest possible model, one row per folder, and it
would have made #711 Instance A disappear without any of this work — the
aggregate row would no longer be superseded because nothing would supersede it.

**Rejected because** it reverts a deliberate earlier decision (spec 041 iterate:
mixed folders resolve into single-type items at ingest, behind a
missing-mandatory gate) that was taken for a reason that still holds. It also
leaves the metadata confirm gate with nothing coherent to gate on for a mixed
folder: the gate asks whether an item's mandatory metadata is present, and for a
row spanning lights and darks the answer is neither yes nor no. The user would
be back to confirming a folder wholesale or not at all, which is what the
single-type work was done to escape.

### Option 2a — Split into siblings, but require the user to re-classify each

Adopt the sibling model, but treat the split output as unclassified and require
the user to state each sibling's frame type.

**Genuine advantage**: Maximally explicit — nothing enters inventory without the
user having affirmed it, and it sidesteps every question about trusting
header-derived classification.

**Rejected because** classification is derived from the files' own headers. The
split exists *because* the machine already determined the frame types; asking
the user to restate them re-does known machine work and imposes effort
proportional to N on a user whose files were readable all along. The
needs-review path already exists for the case where the machine genuinely cannot
tell, which is the case where user input carries information.

### Option 2b — Split into siblings, carrying classification forward *(chosen)*

The derived frame types are carried onto the siblings, which arrive classified.
User input is required only where the machine could not determine a frame type.
This is D-001 through D-003.

### Not considered a real option — patch the read side again

Two attempts have already done this: #1038 hid the aggregate row whenever any
sibling existed, which broke homogeneous folders because for those the aggregate
row is the one confirm and plans bind to; #1081 narrowed it to genuine splits,
which fixed the regression and knowingly returned #711 Instance A for the
unsplit case. #1081's own follow-up notes state that fixing it properly means
making the real items authoritative rather than hiding the row the workflow
depends on. A third read-side patch would leave authority silently flipping
between rows depending on group count, with nothing in the model declaring which
row is authoritative.

## Assumptions

- The existing source-group record already holds the folder-level identity this
  model needs (root, relative path, content signature, format, lane), so no new
  folder-level concept is introduced — verified, see [research.md](research.md)
  §3.
- Sibling coexistence at one folder path is already supported, because the
  current parent and child rows already coexist that way — verified, see
  [research.md](research.md) §3.
- Classification remains derived from file headers, and the needs-review bucket
  remains the mechanism for files whose frame type cannot be determined.
- The lane distinction (move versus catalogue) remains a folder-level property
  of the source group, not a per-item property.
- No change is intended to how plans are reviewed or applied, beyond which item
  a plan binds to.

## Out of Scope

- Any change to how frame types are inferred from headers.
- Any change to plan review, plan application, or the filesystem-mutation
  safety model.
- Any change to the metadata confirm gate's criteria (only to which item it is
  evaluated against).
- Migration of existing inbox data (D-004).

## Relationship to #711 Instance B

#711 reported two directions of the same disagreement. **Instance B** — a row
reading `NEEDS REVIEW` for an item that is in fact a confirmable light frame —
was closed by PR #1086 (`22f94a9e`) while this specification was being written.
That fix re-checks the mandatory-attribute gate before promoting a
sentinel-carrying row, and gates the classification-cache write with the same
check, so the cached classification can no longer flip to `single_type` while
the list row and confirm still correctly see the sentinel.

This feature therefore claims **the unsplit remainder of Instance A only**.
Instance A is the direction caused by the parent row; #1038 and #1081 between
them closed the split case, and the folder that produces exactly one item is
what is left. It is the case no read-side patch has been able to fix without
breaking something else, because for a homogeneous folder the placeholder is the
row the workflow depends on.

Two consequences for planning:

- Instance B's fix must be **preserved**, not regressed. SC-011 makes that
  explicit, and `22f94a9e` ships its own regression test
  (`crates/app/inbox/src/reclassify.rs:1086-1142`).
- #1086 made the `__needs_review__` sentinel *more* load-bearing rather than
  less. **Resolved by Q-7**: the workaround does not survive — needs-review moves
  to its own field (FR-028), and the atomic frame-type/identity/state transition
  that `clear_needs_review_sentinel` performs today must be preserved in its
  replacement (FR-029). The #1086 test's setup and assertions are edited to read
  the new field; **its invariant and SC-011 are unchanged** (FR-030).

## Dependencies

- PR #1081 (`b4e72263`) and PR #1038 (`1eae04e9`) are both merged. This feature
  deletes their shared read-side predicate and the `exclude_split_placeholder!`
  macro (FR-026, SC-007); nothing needs to be sequenced around them.
- PR #1086 (`22f94a9e`) is merged and must be preserved (SC-011).
- A follow-on micro-spec under `specs/tiny/` owns plan invalidation and the
  retirement of the folder-wide reclassify interlock (the former FR-020/021/022).
  It depends on this feature rather than blocking it: 058 ships the interlock
  untouched. See `specs/tiny/reclassify-split-per-item-and-rederivation.md` (PR #1097).
- The confirm staleness guard is **already vacuous on the reclassify path on
  `main`** (Q-5). That defect is independent of this feature and could be fixed
  ahead of it.

## Next Gates

Per the constitution, this specification must pass review before planning.
`plan.md`, `data-model.md`, `contracts/`, and `tasks.md` are deliberately not
included here — they are owned by the plan gate. [research.md](research.md)
records the current-code evidence gathered during specification so the planner
does not have to re-derive it.

All nine review questions are now answered
([PENDING_REVIEW_QUESTIONS.md](PENDING_REVIEW_QUESTIONS.md)). Three items are
handed to the plan gate as **work to size, not decisions to take**:

1. **The D-006 E2E helper tension.** `rescan_and_wait_for_item` and
   `select_only_item` both assume a scan yields exactly one *selectable* item per
   folder, but under D-006 a scan yields only a source group — which this
   specification also requires to be non-confirmable. Five journeys are affected,
   including all three SC-005 journeys. Detailed in
   [PENDING_REVIEW_QUESTIONS.md](PENDING_REVIEW_QUESTIONS.md).
2. **The needs-review split gap** (Q-9): whether a heterogeneous needs-review
   bucket resolved into two frame types should split rather than come to rest in
   `mixed` with Confirm disabled.
3. **The Q-5 signature work**: threading a root absolute path into
   `reclassify_v2`, plus the regression test that proves the confirm guard fires.
