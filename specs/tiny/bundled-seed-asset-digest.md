# TinySpec: Out-of-crate bundled seed blobs are pinned to committed SHA-256 literals

**Branch**: fix/lp3-seed-asset-digest
**Date**: 2026-08-23
**Status**: draft
**Complexity**: small

## What

`crates/targeting/resolver/src/seed.rs` embeds two JSON blobs that live outside the
crate, at `assets/seed/seed.json` (4 764 178 B, ~13 073 entries) and
`assets/seed/seed-e2e.json` (90 490 B). The full blob is produced by a SIMBAD TAP
network fetch in `crates/tools/seed-builder`, and it ships: it is compiled into the
desktop binary and warmed into the user's resolve cache on first run.

`git ls-tree` of `assets/seed/` carries no `.sha256`, no signature and no manifest, so
nothing in the repository asserts what those bytes are. A one-byte change inside a
4.5 MB JSON file is not detectable by a human reading the diff, and `opengrep` skips
files over 1 MB, so no scanner reads it either. The blobs are intact today; what is
missing is the ability to notice if they stop being intact.

## The rule (decision)

Bytes compiled into a shipped artifact from outside the compiling crate are pinned by
a SHA-256 hex literal committed in the test source, and the digest is taken from the
embedded bytes rather than from a runtime file read.

### One digest literal per blob, in the test source

Each embedded blob gets its own `const` hex literal and its own assertion. A shared or
combined digest is not acceptable: it cannot say which blob moved. The literal is a
short line a reviewer reads in the diff.

### The digest is computed, never regenerated

No recipe, `build.rs` or committed `SHA256SUMS` file derives the expected value from
the working tree. A generator run in the same commit as a swapped blob produces a
matching digest and proves nothing. The expected value is only ever changed by a human
editing the literal, which puts the change in the diff.

### The digest covers what ships

The assertion hashes bytes obtained through `include_bytes!` of the same paths the
production loaders use. A path typo then fails at compile time rather than passing
vacuously, and a file re-read at test runtime cannot diverge from the embed.

### An assets-only change reaches the Rust lane

`assets/seed/**` is part of the `rust` paths filter in `.github/workflows/ci.yml`.
Those bytes are compiled into Rust artifacts, so a change there invalidates the Rust
lane, and without the filter entry a pull request touching only the blobs skips the
`integration` job entirely (gate at `.github/workflows/ci.yml:287-289`).

### The blobs are LF on every platform

`.gitattributes` pins `assets/seed/*.json` to `text eol=lf`. A byte digest is
platform-dependent otherwise: `windows-latest` checks out with `core.autocrlf=true`,
the working tree gets CRLF, and the digest mismatches on Windows alone. The digest is
never made to pass by an OS `cfg` gate or by normalizing newlines, both of which would
defeat a byte digest.

## Per-finding verdict at head efaaf57a0d800a91efab5437d6d4eac32cc78ad5

| Bead | Verified site | Reachable with real inputs | Verdict |
| --- | --- | --- | --- |
| `astro-plan-3v3r.19.18` | `crates/targeting/resolver/src/seed.rs:201` (`seed.json`), `:218` (`seed-e2e.json`) | Yes for `:201` — `apps/desktop/src-tauri/src/lib.rs:896` and `apps/desktop/src-tauri/src/resolve_cache.rs:138` call `seed::warm_bundled_on_first_run`, which calls `bundled()` at `crates/targeting/resolver/src/seed.rs:407`. `:218` is reached only by the E2E pre-warm harness. | Confirmed as a review-visibility gap, not a runtime fault. Both blobs hash at this head to the values pinned by this spec, so no tampering has occurred. |

## Context

- Precedent for the shape: `crates/persistence/core/tests/baseline_invariants.rs:111-125`
  freezes the `0001` migration against a committed 48-byte checksum literal.
- Precedent for the `eol=lf` reasoning: `.gitattributes` already pins `*.sql` and
  `*.rs` with the comment that the frozen baseline checksum must be identical on all
  platforms.
- `sha2 = "0.10"` is already a workspace dependency (`Cargo.toml:61`), used as
  `sha2.workspace = true` by five crates.
- Measured digests at this head:

```
assets/seed/seed.json      aa442354ca1f36cd0f56acea209ae19390c479e9affe94e10fd135d09313365a
assets/seed/seed-e2e.json  ec41a79461c76693dbfa67b7526f7178d059f62b931696a0317b0829ab545c9f
```

## Requirements

1. A test in `crates/targeting/resolver` hashes the `include_bytes!` embed of
   `assets/seed/seed.json` with SHA-256 and asserts it equals a committed hex literal.
2. The same test file does the equivalent for `assets/seed/seed-e2e.json` against its
   own separate hex literal.
3. Mutating one byte of either blob fails the corresponding assertion, and the failure
   message names the blob and prints both the expected and actual digests.
4. `assets/seed/**` matches the `rust` filter in `.github/workflows/ci.yml`, so a
   change confined to those files runs the Rust lane.
5. `.gitattributes` pins `assets/seed/*.json` to `text eol=lf`, so requirement 1 and 2
   hold byte-identically on Windows, macOS and Linux.
6. Neither assertion is gated on target OS, and neither normalizes newlines before
   hashing.

## Tasks

- [ ] Add `sha2` to `[dev-dependencies]` of `crates/targeting/resolver/Cargo.toml` as
      `sha2.workspace = true`
- [ ] Add the digest test with one `const` literal and one assertion per blob
- [ ] Add `'assets/seed/**'` to the `rust` paths filter in `.github/workflows/ci.yml`
- [ ] Add `assets/seed/*.json text eol=lf` to `.gitattributes` with its reason
- [ ] Prove non-vacuity: mutate one byte of each blob, observe the failure naming that
      blob, restore, observe the pass
- [ ] `cargo fmt`, `cargo clippy -p targeting_resolver --all-targets`,
      `cargo nextest run -p targeting_resolver`, `cargo machete`

## Done When

- [ ] `cargo nextest run -p targeting_resolver` passes with both digest assertions present
- [ ] A single-byte mutation of either blob fails that blob's assertion and no other
- [ ] `git status --porcelain` is clean for `assets/` before the commit
- [ ] The `rust` filter block in `.github/workflows/ci.yml` contains `'assets/seed/**'`
- [ ] `.gitattributes` contains `assets/seed/*.json text eol=lf`
- [ ] Clippy and fmt are clean for `targeting_resolver`

## Out of scope

Named here so the boundary is explicit, not implied:

- Authenticity and provenance. There is no signature and no repository key, so this
  establishes tamper-evidence in review and nothing more.
- Verification inside `crates/tools/seed-builder`, which still fetches over the network
  unverified. Tracked as `astro-plan-3v3r.10`.
- The other out-of-crate embeds. `crates/metadata/core/src/profile.rs:13` is in-crate
  and hand-authored; `crates/e2e-tests/tests/common/boot.rs:250` is test-only, in-repo,
  hand-authored and ships in nothing. Neither is network-derived.
