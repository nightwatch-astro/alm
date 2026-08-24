# Persistence Layer Hardening

**Status (2026-07-11): COMPLETE — boundary sealed at zero.** The relocation
effort has landed. Every production `sqlx` query/exec site has been drained out
of the app and domain crates into `crates/persistence/db`, and the guard is now
locked at zero: `scripts/db-boundary-baseline.txt` is empty and any new
production SQL outside `persistence/db` fails CI. Delivered across PRs #534,
#536–#544 (the per-crate drains) and #546 (the seal), under the
`db-boundary-zero` orchestration run.

This document is preserved as the research and history behind the effort.
Sections 4–5 (sea-query pattern, sqlx offline verification) remain the reference
record for the chosen mechanisms. Where the original text said "later effort" or
"this branch does not migrate," that work is now done — see the dated notes
inline.

---

## 1. Problem: DB access has leaked out of `persistence/db`

The repo has a dedicated repository crate, `crates/persistence/db`, intended to
be the single home for SQL. In practice, raw `sqlx` calls have spread across the
app and domain crates.

### Audit numbers

Raw-match audit (all `sqlx::query` / `query_as` / `.fetch_*` / `.execute(`
occurrences, including test code):

- **~504 production query sites** + **~1010 test sites** outside `persistence/db`
- across **~35 production files**.

Production-only count, as measured by the ratchet (`scripts/check-db-boundary.sh`,
which excludes `tests/` directories and inline `#[cfg(test)]` modules):

- At scaffolding time: **212 production query sites** across **23 production
  files**. **As of 2026-07-11 this count is 0** — every one of those sites was
  relocated into `crates/persistence/db` and `scripts/db-boundary-baseline.txt`
  is now sealed empty. The audit numbers below are the historical starting
  point; the table in the next section records where the SQL used to live.

> The raw audit count (~504, taken when this scaffolding was first drafted) and
> the ratchet count differ because the ratchet deliberately scopes to
> *production* code only — it excludes integration tests under `*/tests/**` and
> the inline `#[cfg(test)] mod tests` blocks that live at the bottom of many
> production source files. The ratchet count is the meaningful enforcement
> number, and `scripts/db-boundary-baseline.txt` (not this doc) is the source of
> truth for current per-file counts.

### Top offenders (historical — where the SQL used to live, all now drained to 0)

| Sites | File |
| ----: | ---- |
| 38 | `crates/targeting/resolver/src/cache.rs` |
| 28 | `crates/app/targets/src/ingest_sessions.rs` |
| 16 | `crates/app/targets/src/ingest_resolution.rs` |
| 16 | `crates/app/calibration/src/matching.rs` |
| 14 | `crates/app/targets/src/target_management.rs` |
| 12 | `crates/app/core/src/sessions.rs` |
| 12 | `crates/app/core/src/search.rs` |
| 10 | `crates/app/core/src/log_stream.rs` |
| 10 | `apps/desktop/src-tauri/src/commands/status.rs` |
|  8 | `apps/desktop/src-tauri/src/commands/inbox.rs` |

(Full per-file list lives in `scripts/db-boundary-baseline.txt`; treat that file,
not this table, as authoritative — it will drift as unrelated work lands.)

### Stack

- **sqlx 0.9** (sqlite, `runtime-tokio`, `tls-rustls`, `macros`, `migrate`,
  `uuid`, `time`, `json`) — async, ratified in spec 002.
- Canonical store: SQLite. Migrations in `crates/persistence/core/migrations/`,
  applied via `sqlx::migrate!("./migrations")`. Before 1.0 there is exactly one
  migration, `0001_initial_schema.sql`, edited in place.

---

## 2. The 6-part plan

1. **Boundary enforcement.** ✅ **Done.** A CI guard that forbids production SQL
   outside `persistence/db`. Started as a shrink-only ratchet; now **sealed at
   zero** (see §3).
2. **Relocation.** ✅ **Done.** Every production query was moved into a
   `persistence/db` repository method, driving the baseline to zero (PRs #534,
   #536–#544).
3. **Typed rows.** ✅ **Done as part of relocation.** Relocated queries map onto
   `#[derive(sqlx::FromRow)]` row structs in the `persistence/db` repositories
   instead of ad-hoc tuple/`Row` access.
4. **sea-query for dynamic queries (pattern + dep).** Dynamic
   `WHERE`/`ORDER BY`/pagination built with `sea-query`, bound to sqlx 0.9
   manually. Reference pattern in `query_builder_example.rs`.
5. **Compile-time verification (mechanism only).** sqlx offline query cache
   (`.sqlx/`) so static queries are checked against the schema at build time,
   and CI builds with `SQLX_OFFLINE=true`.
6. **Lint ratchet.** ✅ **Done + wired into CI.** The boundary script (+ a
   secondary clippy signal) runs as the `DB boundary ratchet` step in
   `.github/workflows/ci.yml` via `just db-boundary`; the baseline is now sealed
   empty.

---

## 3. Boundary lint (the keystone)

### Why a script, not clippy `disallowed-methods`

clippy's `disallowed-methods` cannot be **path-scoped**. There is no way to say
"disallow `sqlx::query` everywhere *except* `crates/persistence/db`". Enabling it
workspace-wide would (a) fire inside the sanctioned `persistence/db` crate itself
and (b) fire inside every test module — producing hundreds of false positives,
and under CI's `cargo clippy -- -D warnings` it would break the build outright.

So the **authoritative** guard is `scripts/check-db-boundary.sh`, and clippy is a
**secondary, opt-in** signal only.

### How the ratchet works

`scripts/check-db-boundary.sh`:

- Enumerates `*.rs` under `crates/` and `apps/`, **excluding**:
  - `crates/persistence/db/**` (the sanctioned SQL home),
  - any path containing a `tests/` segment (integration tests),
  - `query_builder_example.rs` (the reference module).
- For each remaining file, counts query/exec sites
  (`sqlx::query | query_as | query_scalar | .fetch_(one|all|optional) |
  .execute(`) outside every inline `#[cfg(test)]` item's own brace scope. Unit
  test modules are not production code, but production code *following* one in
  the same file still counts, and a comment or blank line between the attribute
  and the item it applies to does not break the exemption.
- **Now sealed at zero (2026-07-11).** `scripts/db-boundary-baseline.txt` is
  empty, so the check fails (exit 1) on **any** production query site outside
  `persistence/db`. It also rejects a non-empty (hand-edited) baseline
  (`SEAL BROKEN`) so the boundary cannot be silently re-opened, and `--generate`
  refuses to record leakage — new SQL must be added as a `persistence/db`
  repository method.
- *History:* this began as a shrink-only ratchet that compared each file's count
  against a per-file baseline (`count<TAB>path`) and allowed counts only to
  decrease. Once relocation drove every count to 0, the baseline was sealed
  empty and the guard hardened to zero-tolerance.

Commands:

```bash
bash scripts/check-db-boundary.sh            # CI check (exit 1 on ANY production SQL / non-empty baseline)
bash scripts/check-db-boundary.sh --generate # re-seal the empty baseline (refuses to record leakage)
bash scripts/check-db-boundary.sh --list     # print current per-file counts (now empty)
just db-boundary                             # = the CI check
```

Current state: **green, sealed at zero** — 0 files / 0 production query sites
outside `persistence/db` (`scripts/db-boundary-baseline.txt` holds only header
comments).

### clippy.toml (secondary signal)

`clippy.toml` adds `disallowed-methods` entries for `sqlx::query`,
`sqlx::query_as`, `sqlx::query_scalar`. Because the lint cannot be path-scoped,
the workspace holds `clippy::disallowed_methods = "allow"` (in
`[workspace.lints.clippy]`, with the `all`/`pedantic` groups demoted to
`priority = -1` so the per-lint override wins). This keeps CI green on today's
legitimate sites while letting a developer surface the signal on demand:

```bash
cargo clippy -- -W clippy::disallowed_methods
```

### CI wiring — DONE

`just db-boundary` is wired into CI as the **DB boundary ratchet** step in
`.github/workflows/ci.yml`:

```yaml
      - name: DB boundary ratchet
        run: just db-boundary
```

`just` is installed in CI via `taiki-e/install-action`. The step is fast (pure
grep, no compile). It runs on every change so any new production SQL outside
`persistence/db` fails the build.

### Regenerating the baseline (sealed at zero)

The baseline is now locked empty, so regeneration is no longer routine
housekeeping. `--generate` **refuses** to write a non-empty baseline: if any
production query site exists outside `persistence/db`, it prints the offenders
and exits non-zero instead of recording them. The correct fix for a new leak is
to move the query into a `persistence/db` repository method — not to regenerate.

```bash
bash scripts/check-db-boundary.sh --generate  # re-seals the empty baseline; refuses any leak
```

`scripts/db-boundary-baseline.txt` remains the source of truth; it should stay
empty (header comments only). The tables in this document are historical
snapshots.

---

## 4. sea-query: dynamic query builder

### Research finding: do NOT use `sea-query-binder`

`sea-query-binder` is the "official" bridge from sea-query to sqlx, but it is
**incompatible with our stack**:

- Its current release (`0.8.0-rc.7`) pins `sqlx = "0.8"` in its manifest
  (verified: `[dependencies.sqlx] version = "0.8"`).
- We are on the ratified **sqlx 0.9**. Pulling in the binder drags a *second*,
  incompatible sqlx into the dependency tree (`cargo tree` shows both
  `sqlx v0.8.6` via the binder and `sqlx v0.9.0` from our workspace).
- Its bound `SqliteArguments` (sqlx 0.8) would not feed a sqlx 0.9
  `query_with`, so it cannot bridge our stack at all.

### Chosen pattern: `sea-query` standalone + manual binding

`sea-query` (1.0.1, latest stable; MSRV 1.88 — fine on our toolchain 1.95) is a
**pure** query builder with **no sqlx dependency**. We add only:

```toml
sea-query = { version = "1", default-features = false, features = ["derive", "backend-sqlite"] }
```

The canonical pattern (`crates/persistence/db/src/query_builder_example.rs`):

1. Build the statement with `sea-query`, producing parameterized SQL plus a
   `Values` payload:

   ```rust
   let (sql, values) = stmt.build(sea_query::SqliteQueryBuilder);
   ```

   sea-query emits `?` placeholders and **never** interpolates user values into
   the SQL text — values travel out-of-band in `Values`.

2. Hand the SQL to sqlx 0.9. Because the string is built at runtime (not a
   literal), sqlx 0.9 requires an explicit safety opt-in via `AssertSqlSafe`
   (see §5.1):

   ```rust
   let query = sqlx::query_as::<_, WidgetRow>(sqlx::AssertSqlSafe(sql));
   ```

3. Fold the `Values` onto the query by matching each `sea_query::Value` variant
   and calling `.bind(...)` with the corresponding Rust type. The example
   provides `bind_values` (untyped) and `bind_values_as` (typed `FromRow`)
   helpers handling the variants we actually use (bool, the int family, floats,
   `String`, `Char`, `Bytes`, all `Option`-wrapped). Exotic variants are
   `unimplemented!()` with a clear message so accidental use is loud.

4. Map results into a `#[derive(sqlx::FromRow)]` struct.

The example is gated `#[cfg(test)]` (compiled only under test, never shipped in
release, never wired into a real repository) and is covered by two passing smoke
tests: a dynamic SELECT with two optional WHERE clauses that round-trips against
in-memory SQLite, and a dynamic INSERT via the untyped binder.

> Decision recorded: **sea-query builds SQL + Values; we bind to sqlx 0.9
> ourselves.** No `sea-query-binder` (it lags sqlx). This keeps us on latest
> stable sea-query + the ratified sqlx 0.9 with zero pre-release deps.

Notable sea-query 1.0 API points discovered while writing the example:

- Comparison combinators (`eq`, `gte`, …) live on the `ExprTrait` trait, which
  must be imported (`use sea_query::ExprTrait;`).
- Column/table identifiers use `#[derive(sea_query::Iden)]` enums.

---

## 5. sqlx compile-time verification (offline)

This branch set up the **mechanism only**, and it remains unadopted. Note
(2026-07-11): the relocation (part 2) moved queries into `persistence/db` using
**runtime** `sqlx::query_as` + `#[derive(sqlx::FromRow)]` row structs, **not**
the compile-time `query!`/`query_as!` macros. So there is still no `.sqlx/`
offline cache and CI does not set `SQLX_OFFLINE`. Converting the runtime queries
to checked macros (and populating `.sqlx/`) is a possible future step; the
zero-boundary seal does not depend on it.

### 5.1 sqlx 0.9 dynamic-SQL guard (`AssertSqlSafe`)

A migration-relevant sqlx 0.9 change: `query`/`query_as` now accept
`impl SqlSafeStr`. A `&'static str` literal is implicitly safe; a runtime
`String` is **not** and must be wrapped:

```rust
use sqlx::AssertSqlSafe;            // re-exported from sqlx_core::sql_str
sqlx::query_as::<_, Row>(AssertSqlSafe(sql))   // sql: String built by sea-query
```

This is sound for sea-query output because the statement text is parameterized
and contains no user data. Any relocated dynamic query will use this.

### 5.2 Offline query cache workflow

Static `query!`/`query_as!` macros verify SQL against the live schema at compile
time. To avoid requiring a live DB during normal/CI builds, sqlx supports an
**offline cache** (`.sqlx/`):

1. Generate the cache from a migrated database:

   ```bash
   cargo install sqlx-cli --no-default-features --features sqlite   # one-time
   just sqlx-prepare        # = cargo sqlx prepare --workspace -- --all-targets
   ```

   `sqlx prepare` needs either a `DATABASE_URL` env var pointing at a migrated
   SQLite database, or a database the crate's migrations have been applied to.

2. **Commit** the resulting `.sqlx/` directory. It is intentionally NOT
   git-ignored (noted in `.gitignore`).

3. CI and offline builds set `SQLX_OFFLINE=true`, which makes the macros read
   `.sqlx/` instead of connecting to a database:

   ```yaml
   env:
     SQLX_OFFLINE: "true"
   ```

   Add this to the workspace build/test jobs once `query!` macros are actually
   in use. (No effect today, since no macro queries exist yet.)

### 5.3 Sequencing the relocation effort

The offline cache encodes the schema, so it must be regenerated whenever
migrations change — run `just sqlx-prepare` against the latest migrated schema,
not a stale one. Sequence for the migration effort:

1. Regenerate the boundary baseline (`§3`) against the current `main`/base
   branch immediately before starting relocation work.
2. Begin relocating queries into `persistence/db`, converting static ones to
   `query!`/`query_as!` macros.
3. `just sqlx-prepare`, commit `.sqlx/`, enable `SQLX_OFFLINE=true` in CI once
   the workflow step from §3 exists.

---

## 6. What this branch delivered (additive only)

- `docs/development/persistence-layer-hardening.md` (this file).
- `scripts/check-db-boundary.sh` + generated `scripts/db-boundary-baseline.txt`.
- `clippy.toml`: `disallowed-methods` entries (secondary signal) + workspace
  `disallowed_methods = "allow"` + group priority fix.
- `Cargo.toml`: `sea-query` workspace dep (+ rationale comment).
- `crates/persistence/db/Cargo.toml`: `sea-query.workspace = true`.
- `crates/persistence/db/src/query_builder_example.rs`: reference pattern
  (`#[cfg(test)]`, not wired into repos) + smoke tests.
- `justfile`: `db-boundary` and `sqlx-prepare` recipes.
- `.gitignore`: note that `.sqlx/` is intentionally committed.

No existing query was migrated, converted, or relocated *by that scaffolding
branch*. The relocation itself landed later, under the `db-boundary-zero` run
(PRs #534, #536–#544, sealed by #546) — see the status banner at the top.

---

## 7. Resolved: inline test SQL stays inline

Many production source files carry their unit tests inline as
`#[cfg(test)] mod tests` at the bottom of the same `.rs` file, and those test
modules held the bulk (~1010) of the query sites. The ratchet excludes them by
design (they are not production code).

**Resolution (2026-07-11):** the relocation moved only **production** SQL into
`persistence/db`; inline `#[cfg(test)]` fixture SQL was **left as-is**. Test
modules legitimately set up fixtures with raw `sqlx`, and the boundary is scoped
to production code, so those sites remain and are correctly ignored by the guard.
The zero-boundary seal counts only production sites.
