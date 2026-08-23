# TinySpec: An out-of-range stored value falls back to its documented default, never to a clamped edge or a fabricated date

**Branch**: fix/dbs5-documented-fallbacks
**Date**: 2026-08-23
**Status**: draft
**Complexity**: small

## What

Two families of read-side fallback in this repo declare a default that the code
cannot reach, because a clamp runs first and produces an in-range value the
declared default never sees.

- `crates/app/targets` and one `apps/desktop/src-tauri` line read `i64` resolver
  settings, clamp with `.max(0)` / `.max(1)`, then `try_from` into `u32`/`u64`
  with `.unwrap_or(300)` / `.unwrap_or(10)`. A stored `-1` reaches the caller as
  `0` or `1`, never as `300` or `10`.
- `crates/sessions` derives the observing night twice. `key.rs` fabricates a date
  on pre-noon underflow (`previous_day().unwrap_or(d)`); `observing_night.rs`
  refuses it (`ok_or(DateUnderflow)`).

## The rule (decision)

### A non-positive or unrepresentable stored resolver setting is not a setting

A stored `debounce_ms` or `request_timeout_secs` that is `<= 0`, or larger than
the target integer type, MUST yield the documented default — `300` ms and `10`
seconds — and MUST NOT yield a clamped edge value. A zero debounce disables
throttling on the SIMBAD path and a zero timeout fails every request, so both
are refusals, not values to be repaired by clamping.

One helper implements this rule for all four read sites. The write side is
unchanged: `resolver_settings::update` keeps clamping user input to `>= 1`.

### One observing-night date producer, and it refuses rather than fabricates

`observing_night::noon_bounded_date` is the single derivation of the
noon-bounded date. `key::observing_night` MUST obtain its date from that
function and MUST propagate the underflow as an error. `key.rs` MUST NOT hold a
second derivation, and no derivation may substitute an unshifted date for a date
it cannot compute.

### The persisted session-key night format is frozen

`key::format_date_iso`'s `{:04}` year padding is unchanged. That string is the
`night` segment of `SessionKey`, persisted on `acquisition_session.session_key`
and read back by `SessionKey::parse`. Changing the padding would orphan existing
session rows. The padding divergence the finding reports disappears because
there is one date producer, not because the format changed.

## Per-finding verdict at head `5b1fdf702f7ad12d52ea429a255cc69874ef5a2b`

| Bead | Site at this head | Reachable with real inputs | Verdict |
| --- | --- | --- | --- |
| `astro-plan-3v3r.11.24` | `crates/app/targets/src/resolver_settings.rs:85` (`debounce_ms`) | NO | fix as unreachable hardening |
| `astro-plan-3v3r.11.24` | `crates/app/targets/src/resolver_settings.rs:86` (`request_timeout_secs`) | NO | same defect, not named in the bead; fixed |
| `astro-plan-3v3r.11.24` | `crates/app/targets/src/ingest_resolution.rs:374` | NO | same shape, not named in the bead; fixed |
| `astro-plan-3v3r.11.24` | `apps/desktop/src-tauri/src/commands/target_lookup.rs:130` | NO | same shape, not named in the bead; fixed |
| `astro-plan-3v3r.13.31` | `crates/sessions/src/key.rs:138` | NO | fabrication removed; duplicate derivation collapsed |
| `astro-plan-3v3r.13.31` | `crates/sessions/src/observing_night.rs:102` | n/a — this is the correct behaviour | kept as the single producer |

Why unreachable:

- **11.24.** The only writer of the `resolver_settings` row is
  `q_targets_mgmt::upsert_resolver_settings`, called from exactly one site,
  `resolver_settings.rs:132`, which passes `i64::from(s.debounce_ms.max(1))`
  from a contract `u32` (`crates/contracts/core/src/targets.rs`). The stored
  value is therefore always in `1..=u32::MAX`. The column carries no `CHECK` and
  the table is not `STRICT`, so an out-of-range value is storable only by an
  out-of-band database edit or corruption.
- **13.31.** The underflow needs a pre-noon local timestamp on `Date::MIN`
  (`-9999-01-01`). Every `capture_at` reaching `sessions::observing_night` comes
  from `ingest_sessions::parse_date_obs`, which parses with `Iso8601::DEFAULT`.
  The workspace does not enable the `time` crate's `large-dates` feature
  (`Cargo.toml:116`), so a signed expanded year is not a parseable ISO 8601 year
  and the function falls back to `OffsetDateTime::now_utc()`.

The observing-night divergence was never observable through a session key: no
production path constructs an `ObservingNight`. Its only constructor call
outside its own module is a `#[cfg(test)]` helper at
`crates/sessions/src/identity.rs:266`; the production type that holds one
(`identity.rs:216`) is never built with a real value. The finding's claim that
grouping and lookup disagree for the same frame does not hold at this head. The
defect being fixed is the duplicate derivation itself, one half of which
fabricates a date where the other refuses.

## Context

- `read_row` (`resolver_settings.rs:73-90`) is read-through a process-wide
  snapshot cache. A test asserting a value from it must invalidate the cache and
  serialize against the other resolver-settings tests via
  `target_management::cache_test_lock`.
- `defaults()` (`resolver_settings.rs:28-35`) also returns `300`/`10`, so a test
  that fails to seed the singleton row passes vacuously. Seeding must be
  asserted before the value is.
- `key::observing_night` gains an error arm. Both call sites already discard the
  error: `ingest_sessions.rs:377` with `.ok()` and `:499` with
  `unwrap_or_else`.
- Migration `0001_initial_schema.sql` is frozen by a checksum test. No migration
  is added: a `CHECK` constraint would not apply to the existing table without a
  rebuild, and the read-side guard is the fix.

## Requirements

1. A stored `debounce_ms` of `-1` reads back as `300`.
2. A stored `debounce_ms` of `0` reads back as `300`.
3. A stored `request_timeout_secs` of `-1` reads back as `10`.
4. A stored `request_timeout_secs` of `0` reads back as `10`.
5. The helper yields `10` for the stored `0` and `-1` that the
   `ingest_resolution` and `target.lookup` timeout sites pass it, not `1`.
   `SimbadConfig` is an external type with no timeout accessor, so the timeout
   those two sites hand it is not observable from a test; the helper call is the
   assertable boundary.
6. `resolver_settings::update` still clamps a submitted `0` to `1`.
7. A single helper implements requirements 1-5 for all four read sites.
8. `key::observing_night` returns an error for a pre-noon timestamp on
   `Date::MIN`, rather than the string `-9999-01-01`.
9. `key::observing_night` returns the unshifted date for an at-or-after-noon
   timestamp on `Date::MIN`.
10. `key::observing_night` and `ObservingNight::from_acquisition_timezone` name
    the same date for `-0001-01-01T00:00 +00:00`.
11. `key.rs` holds no date-shifting helper of its own.
12. `format_date_iso`'s output is unchanged for every year width.
13. An evidence test records whether `parse_date_obs` parses
    `-9999-01-01T00:00:00` as year `-9999`. If it ever does, requirement 8's
    defect is reachable from a hand-edited FITS header and this spec's verdict
    table is wrong.

## Tasks

- [ ] Add the out-of-range-to-default helper in `crates/app/targets` with the
      refusal invariant in its docstring
- [ ] Apply it at `resolver_settings.rs:85`, `:86`, `ingest_resolution.rs:374`,
      and `target_lookup.rs:130`
- [ ] Add an underflow arm to `KeyError` and route `key::observing_night`
      through `observing_night::noon_bounded_date`
- [ ] Delete `key.rs`'s private `previous_day`
- [ ] Add the tests for requirements 1-6 and 8-13, each confirmed failing at
      base where it targets a defect
- [ ] `cargo test -p sessions -p app_core_targets`, `cargo check -p desktop_shell`,
      `cargo clippy -p sessions -p app_core_targets --all-targets`

## Done When

- [ ] Every requirement above has a test, and each test that targets a defect
      was recorded failing at base with its actual output
- [ ] `crates/sessions` contains exactly one noon-bounded date derivation
- [ ] The four read sites call one helper, not four copies of one expression
- [ ] `format_date_iso` and migration files are untouched
- [ ] Touched-crate test, check, and clippy gates are green
