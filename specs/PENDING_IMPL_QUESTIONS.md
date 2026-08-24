# Pending Implementation Questions — Spec 002 Foundation

**Generated**: 2026-05-22 (autonomous step 7 of the SpecKit follow-up plan)
**Status**: closed 2026-05-23 — all of Q1–Q8 are ratified in
[Resolution (2026-05-23)](#resolution-2026-05-23) at the foot of this file, and the ratified
answers shipped. This is a historical record, not a live blocker on Rust work; the question
text below states the options as they stood on 2026-05-22 and its recommendations were in
several cases *not* the answer taken. Read the Resolution table first.

## Context

After the ratification + envelope sweep + spec 018 absorption + CLAUDE.md ripple (commits `48cfa35` and `c94384c`, both pushed to `origin/main`), the next step was "begin spec 002 SpecKit implementation". The existing Rust scaffolding in `crates/` is already substantial (~1,569 lines across six crate skeletons), but several material architectural decisions are needed before new code can land safely. Listed below in order of blast radius.

---

## Q1. SQLite driver: `rusqlite` (current) vs `sqlx` (spec 002 task T003)

**The conflict.** Workspace `Cargo.toml` declares `rusqlite = "0.37"` and `crates/persistence/db/Cargo.toml` consumes it. The hand-rolled `MigrationRunner` in `crates/persistence/db/src/lib.rs` is built on `rusqlite::Transaction`. Meanwhile, `specs/002-data-lifecycle-state-model/tasks.md` T003 says:

> Add `sqlx` (sqlite, runtime-tokio-rustls, macros) to `crates/persistence/db/Cargo.toml`.

These are incompatible: rusqlite is sync; sqlx is async + tokio. Picking one closes off the other.

**Tradeoffs:**

| | rusqlite (keep) | sqlx (per task) |
|---|---|---|
| Async | sync; spawn_blocking when called from async | async-native |
| Compile time | fast | slow (sqlx macros invoke the DB) |
| Migration tooling | hand-rolled MigrationRunner already exists | sqlx-migrate or refinery |
| Tauri integration | sync repos behind `tokio::task::spawn_blocking` | direct from async Tauri commands |
| Type safety | runtime (params + extract) | compile-time (with `query!` macro) |
| Workspace status | ✅ wired, working, builds | not yet a dependency |

**Recommendation:** Keep **rusqlite**. The existing scaffolding is intentional; the task.md mention of `sqlx` is outdated planning. Desktop apps don't benefit from async DB drivers (a single-process app with one user has no contention to gain from), and the rusqlite migration runner is clean. If we want compile-time type safety later, we can layer `sqlx::query!` over rusqlite via `sqlx-query-as` or similar.

**What needs to happen if "keep rusqlite":** Amend `specs/002-data-lifecycle-state-model/tasks.md` T003 to say `rusqlite` instead of `sqlx`. Also update spec 002 `plan.md` Technical Context if it references sqlx.

---

## Q2. Migration tooling

**Current state.** `crates/persistence/db/src/lib.rs` has a hand-rolled `MigrationRunner` taking `Vec<Migration>` with `id`, `description`, `sql` static strings. Embedded migrations via `include_str!`. Applied IDs tracked in a `migrations` SQLite table.

**Tradeoffs:**

- **Keep hand-rolled** (current): zero dependencies, predictable, easy to test, already works. Migration files live as `.sql` siblings to `lib.rs` and are `include_str!`'d.
- **refinery**: more featureful (down migrations, dynamic discovery), but adds a dependency and another mental model.
- **sqlx-migrate**: only if Q1 picks sqlx.

**Recommendation:** Keep hand-rolled. We don't need down-migrations for a greenfield local-first app; we need fast compile + clear audit.

**If accepted:** No action — current scaffolding stays.

---

## Q3. Async runtime for Tauri + event bus

**Context.** Tauri 2.x uses tokio. Spec 002's event bus (research.md §6) is core to state propagation across the app. Spec 010's guided flow subscribes; spec 019's log viewer subscribes; specs 014/017/024 publish.

**The decision:** Use `tokio` as the canonical async runtime. The event bus implementation:

- **Option A — tokio `broadcast::channel`**: lossy when receivers are slow (oldest events dropped). Simplest. Good for diagnostic logs, bad for audit-critical events.
- **Option B — tokio `mpsc` with a fan-out worker**: bounded backpressure; receivers must drain. Custom fan-out logic.
- **Option C — SQLite-backed durable bus**: write events to an `events` table; subscribers poll or use a notify trigger. Survives restarts; replay path is the same as initial subscription. Slower than in-memory but durable.
- **Option D — Hybrid**: in-memory broadcast for the "live" channel; SQLite for the "replay" channel. Subscribers can opt in to either.

**Recommendation:** **Option D (hybrid)**. The spec 002 amendment already adds `source: "user" | "restore" | "system"` to the event envelope precisely to support replay-from-audit (spec 010 R-Source-1). The in-memory broadcast covers live UI updates; SQLite-backed replay covers restart recovery. Cost: two surfaces to maintain. Benefit: audit guarantees the constitution requires (every event is durable; no event is lost on restart).

**What needs to happen:** Spec 002 `plan.md` should add an "Event bus runtime" section with this decision. Add `tokio = { version = "1", features = ["sync", "rt-multi-thread", "macros"] }` to workspace dependencies.

---

## Q4. JSON Schema → Rust DTO generation

**Context.** Contracts in `specs/<NNN>/contracts/*.json` are JSON Schema 2020-12. Rust DTOs in `crates/contracts/core/` mirror them. Spec 002 task T012 calls for hand-written DTOs with serde derives. As the contract set grows (~80 contracts now), hand-maintenance gets expensive.

**Options:**

- **Hand-written + a snapshot test that validates request fixtures** (current default): full control, type names match domain vocabulary. Maintenance cost grows linearly with contract count.
- **typify** (https://github.com/oxidecomputer/typify): generates Rust types from JSON Schema as a build script. Output is sometimes ugly but mechanical.
- **schemars (reverse direction)**: writes Rust types, generates the JSON Schema from them. Inverts the source of truth — contracts become *generated* rather than canonical. Constitutionally problematic since spec 002 declares the JSON contract as canonical.
- **quicktype**: language-agnostic but external CLI; not Rust-native.

**Recommendation:** **Hand-written + fixture validation**, until contract count exceeds ~150. The constitutional anchor is the JSON Schema; Rust DTOs are a typed convenience layer. Hand-writing them keeps the type names domain-appropriate and the file diff readable. Add JSON-Schema-fixture-against-Rust-DTO tests (T016 in tasks.md) so the two stay in sync.

**What needs to happen:** Confirm hand-written approach in spec 002 `plan.md`. Optionally evaluate `typify` for a single subtree (e.g., `lifecycle.transition.json`) to validate the build-script integration before adopting broadly.

---

## Q5. Rust → TypeScript type codegen for the desktop adapter

**Context.** Tauri commands cross the Rust ↔ TypeScript boundary. The desktop app currently uses hand-written TS types in `apps/desktop/src/data/` (mock). Production needs typed Tauri command invocations.

**Options:**

- **`ts-rs`**: derive `TS` on Rust types; generates `.ts` files at test time. Most popular. Output is clean. Doesn't handle every edge case (e.g., `Option<T>` ↔ `T | null` quirks).
- **`typeshare`**: same idea, slightly different syntax. Annotations live in `#[typeshare]` attrs.
- **`specta`**: Tauri-specific. Newer. Tighter Tauri integration; supports schema generation directly from Tauri command signatures.
- **Hand-written TS types**: full control, double maintenance.

**Recommendation:** **`specta`**. It's purpose-built for Tauri 2.x, integrates with `tauri-specta` for command type generation, and aligns with the project's Tauri-first stance. Single-purpose tool, narrow blast radius. Output goes into `apps/desktop/src/bindings/` or similar.

**What needs to happen:** Add `specta` to workspace deps; spec 002 `plan.md` should reference the bindings layout.

---

## Q6. Workspace Cargo dependencies expansion

The above decisions imply adding several deps to workspace `Cargo.toml`:

- `tokio = { version = "1", features = ["sync", "rt-multi-thread", "macros"] }` (Q3)
- `specta = "..."` + `tauri-specta = "..."` (Q5)
- *(reject `sqlx`)* (Q1)
- *(no migration crate; keep hand-rolled)* (Q2)
- *(no JSON-schema codegen; hand-written DTOs)* (Q4)

Versions should be checked via `mcp-package-version` per the project rule. The pre-commit hook may also flag any version drift.

**What needs to happen:** Confirm the dep list; I'll run `mcp-package-version` and stage the workspace `Cargo.toml` update.

---

## Q7. Spec 002 task T003 amendment (sqlx → rusqlite)

If Q1 confirms rusqlite, then spec 002 tasks.md needs a small amendment:

- T003: "Add `sqlx` (sqlite, runtime-tokio-rustls, macros) ..." → "Confirm `rusqlite` is wired in workspace + crate Cargo.toml; no additional driver needed."

Also `plan.md` Technical Context if it references sqlx. And the GRILL_DECISIONS amendment block should reflect this.

---

## Q8. Spec 002 review findings from the adversarial re-review

The adversarial re-review (`specs/PENDING_REVIEW_QUESTIONS.md`) flagged a few spec 002 items not yet addressed:

- §6.3 plan-lifecycle event-bus topics need to be added to spec 002's research.md (mechanical; was flagged from spec 017+025 amendment)
- spec 005's stale `DarkFlat` row in the IMAGETYP normalization table contradicts spec 007 R-DarkFlat-Reserved (mechanical fix)

These are mechanical and not architectural — I can fold them in autonomously if you'd like before spec 002 implementation begins.

---

## Suggested order to resolve

1. **Q1 + Q7** (rusqlite, amend task T003): unblocks foundational work.
2. **Q3** (tokio + hybrid event bus): unblocks the event bus implementation.
3. **Q5** (specta for TS bindings): unblocks the Tauri command layer.
4. **Q6** (workspace deps): mechanical once Q1/Q3/Q5 are answered.
5. **Q4** (hand-written DTOs): low blast radius; can be revisited if the count grows.
6. **Q2** (migration tooling): no action — keep hand-rolled.
7. **Q8** (re-review mechanical fixes): can be folded in any time.

Once Q1 + Q3 + Q5 are answered, I can scaffold the foundational lifecycle types (spec 002 Phase 2 T006–T012) in one focused subagent without further input.

---

## What lands when you're back

Read this file, the `specs/PENDING_REVIEW_QUESTIONS.md`, and pick a path. I'll resume with whatever you ratify.

---

## Resolution (2026-05-23)

All Q1–Q8 and the flat-gain dimension question are ratified. Final answers:

| Question | Decision |
|---|---|
| Q1: SQLite driver | **sqlx** (async; `sqlite + runtime-tokio-rustls + macros + migrate` features). Replace rusqlite-based scaffolding during spec 002 Phase 2. |
| Q2: Migration tooling | **Defer** (greenfield). Use `sqlx::migrate!()` with inline `*.sql` files when the migration is written; no separate runner. |
| Q3: Async runtime + event bus | **tokio** runtime. **Hybrid event bus**: `tokio::sync::broadcast` (live) + SQLite `events` table (durable replay). |
| Q4: JSON Schema generation | **Rust-canonical via schemars**. Rust DTOs in `crates/contracts/core/` (with `#[derive(JsonSchema)]`) are the source of truth. JSON files at `specs/*/contracts/*.json` are reproducible projections. `cargo run --bin generate-contracts` regenerates them; CI gates with `git diff --exit-code`. |
| Q5: Rust→TS codegen | **specta + tauri-specta**. Output to `apps/desktop/src/bindings/`. |
| Q6: Workspace deps | sqlx, tokio (sync+rt-multi-thread+macros), schemars, specta, tauri-specta. |
| Q7: T003 amendment | T003 updated to sqlx (already reflected in tasks.md; matches ratification). |
| Q8: Re-review mechanical items | Applied this session (spec 005 DarkFlat row removed; spec 002 §6.3 plan topics added). |
| Flat gain Hard vs Soft | **Hard (exact match)**. Code-fixed. No user setting. `calibration.flat.gain.tolerance_hard` dropped from spec 007 data-model.md and spec 018. |

**Existing rusqlite scaffolding** in `crates/persistence/db/` and `crates/domain/core/` will be replaced during spec 002 Phase 2 Rust scaffolding (task #16).

**Next**: Task #16 — Scaffold spec 002 Phase 2 foundation in Rust (T001–T012 + T010b).
