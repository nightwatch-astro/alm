#!/usr/bin/env node
// Spec 007 T040 — parity guard for the `CalibrationType` enum.
//
// The calibration type domain is duplicated across four places: the Rust
// contract enum and the three suggest/assign JSON-Schema contracts. Nothing
// stopped them drifting apart until this test existed.
//
// Validates that:
// - all three spec-007 contracts define `$defs.CalibrationType`;
// - each one enumerates exactly `dark`, `flat`, `bias`;
// - `contracts_core::calibration_match::CalibrationType` enumerates the same
//   three variants, parsed out of the Rust source;
// - `dark_flat` appears in none of them (FR-001 / R-DarkFlat-Reserved reserves
//   the name in the domain enum but never exposes it in v1 contracts).
//
// T040 originally read "…matches the canonical definition in spec 002 when
// spec 002 adds it" and was deferred on that condition. Spec 002 never added a
// CalibrationType enum — it has no calibration type surface at all — so the
// canonical definition is the Rust contract enum, and that is what this guards.
// If spec 002 ever does add one, add its schema to CONTRACTS below.
//
// Pure Node — no Vitest. Run via
// `node packages/contracts/tests/calibration.type.parity.test.mjs`.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../../..");

const CANONICAL = ["dark", "flat", "bias"];
const RESERVED_NEVER_EXPOSED = "dark_flat";

const CONTRACTS = [
  "specs/007-calibration-matching-rules/contracts/calibration.match.suggest.json",
  "specs/007-calibration-matching-rules/contracts/calibration.match.suggest.batch.json",
  "specs/007-calibration-matching-rules/contracts/calibration.match.assign.json",
];

const RUST_ENUM_SOURCE = "crates/contracts/core/src/calibration_match.rs";

const failures = [];
function assert(cond, msg) {
  if (!cond) failures.push(msg);
}

function sameSet(actual, expected) {
  if (!Array.isArray(actual) || actual.length !== expected.length) return false;
  return expected.every((v) => actual.includes(v));
}

// 1. The three JSON-Schema contracts.
for (const rel of CONTRACTS) {
  const schema = JSON.parse(readFileSync(resolve(repoRoot, rel), "utf8"));
  const def = schema.$defs?.CalibrationType;
  assert(def, `${rel}: $defs.CalibrationType is defined`);
  if (!def) continue;
  assert(
    sameSet(def.enum, CANONICAL),
    `${rel}: CalibrationType enum is [${CANONICAL.join(", ")}], got [${(def.enum ?? []).join(", ")}]`,
  );
  assert(
    !JSON.stringify(schema).includes(`"${RESERVED_NEVER_EXPOSED}"`),
    `${rel}: ${RESERVED_NEVER_EXPOSED} is never exposed in a v1 contract`,
  );
}

// 2. The Rust contract enum, parsed from source. Serde renames the variants
//    snake_case, so `Dark` on the wire is `dark`.
const rustSource = readFileSync(resolve(repoRoot, RUST_ENUM_SOURCE), "utf8");
const enumBody = rustSource.match(/pub enum CalibrationType \{([^}]*)\}/);
assert(enumBody, `${RUST_ENUM_SOURCE}: pub enum CalibrationType found`);
if (enumBody) {
  const variants = enumBody[1]
    .split("\n")
    .map((line) => line.replace(/\/\/.*$/, "").trim().replace(/,$/, ""))
    .filter(Boolean)
    .map((variant) => variant.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase());
  assert(
    sameSet(variants, CANONICAL),
    `${RUST_ENUM_SOURCE}: CalibrationType variants are [${CANONICAL.join(", ")}], got [${variants.join(", ")}]`,
  );
}

if (failures.length > 0) {
  console.error("FAIL");
  for (const f of failures) console.error(" -", f);
  process.exit(1);
}
console.log(`OK — CalibrationType parity across ${CONTRACTS.length} contracts + Rust enum`);
