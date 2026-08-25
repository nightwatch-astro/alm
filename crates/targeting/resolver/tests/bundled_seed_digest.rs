//! Pins the two out-of-crate seed blobs embedded by `src/seed.rs` to committed
//! SHA-256 literals, so a byte change inside a multi-megabyte JSON file is
//! visible in review instead of invisible.
//!
//! Two invariants make this non-vacuous and must survive any edit here:
//!
//! - The bytes come from `include_bytes!` of the same paths the production
//!   loaders use, so a wrong path fails at compile time and the digest covers
//!   what actually ships rather than a file re-read at test runtime.
//! - The expected values are literals a human edits. Deriving them from the
//!   working tree (a recipe, a `SHA256SUMS` file, a `build.rs`) would give a
//!   swapped blob a matching digest in the same commit.
//!
//! No OS `cfg` gate and no newline normalization: `.gitattributes` pins
//! `assets/seed/*.json` to `eol=lf` so these digests hold on every platform,
//! and normalizing bytes before hashing would defeat a byte digest.

use sha2::{Digest, Sha256};

const SEED_JSON: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/seed/seed.json"));

const SEED_E2E_JSON: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/seed/seed-e2e.json"));

const SEED_JSON_SHA256: &str = "aa442354ca1f36cd0f56acea209ae19390c479e9affe94e10fd135d09313365a";

const SEED_E2E_JSON_SHA256: &str =
    "ec41a79461c76693dbfa67b7526f7178d059f62b931696a0317b0829ab545c9f";

fn assert_pinned(path: &str, bytes: &[u8], expected: &str) {
    let actual = format!("{:x}", Sha256::digest(bytes));
    assert_eq!(
        actual, expected,
        "{path} no longer matches its pinned digest\n  expected: {expected}\n  \
         actual:   {actual}\nEither the committed blob changed or the literal in this \
         test is stale. Confirm the blob change was intended before updating the literal."
    );
}

#[test]
fn bundled_seed_matches_pinned_digest() {
    assert_pinned("assets/seed/seed.json", SEED_JSON, SEED_JSON_SHA256);
}

#[test]
fn bundled_e2e_seed_matches_pinned_digest() {
    assert_pinned("assets/seed/seed-e2e.json", SEED_E2E_JSON, SEED_E2E_JSON_SHA256);
}
