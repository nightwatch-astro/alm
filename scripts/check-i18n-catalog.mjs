#!/usr/bin/env node
// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * i18n catalog lint (spec 046 follow-up, run-jc0717 i18n-guard hardening).
 * Cheap, JSON-only checks over apps/desktop/messages/en-GB.json that the
 * alm/no-user-string ESLint rule cannot see (it only inspects src/*.tsx call
 * sites, not the catalog's own values):
 *
 *   1. code-param — a message value interpolates the raw error-code
 *      parameter "{code}" (e.g. "Update failed ({code})."). Error codes are
 *      machine identifiers; showing one to the user verbatim, instead of
 *      mapping it through the i18n error-code registry, is exactly the
 *      anti-pattern spec 046 exists to remove. New instances are rejected;
 *      existing ones (drained by the in-flight error-code migration) are
 *      grandfathered in the baseline.
 *   2. dup-value — two or more keys carry the EXACT same value (after trim +
 *      lowercase). A duplicated single short word (column headers, pane
 *      titles — e.g. "Sort", "All") is the catalog's intentional per-screen
 *      convention and is never flagged: only values that are multi-word OR
 *      longer than DUP_MIN_LENGTH characters count. New duplicate groups are
 *      rejected (reuse the existing key, typically a common_* one, instead
 *      of hardcoding the same prose again); existing groups are
 *      grandfathered. This is exact-match only (no semantic/near-duplicate
 *      detection) — deliberately, to keep the check cheap and the signal
 *      unambiguous.
 *
 * Baseline: scripts/i18n-catalog-baseline.txt, one `<check>\t<signature>`
 * line per grandfathered violation (repo precedent: check-db-boundary.sh /
 * db-boundary-baseline.txt). CI fails on any violation whose signature is
 * NOT in the baseline. A baseline entry that no longer reproduces (the
 * underlying message was fixed) is simply unused — it does not fail the
 * build; shrink the baseline by hand (or re-run --generate) as violations
 * drain in the refactor sweep.
 *
 * Usage:
 *   node scripts/check-i18n-catalog.mjs             # enforce (CI mode)
 *   node scripts/check-i18n-catalog.mjs --generate   # rewrite the baseline to match current violations
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(here, '..');
const CATALOG_PATH = path.join(REPO_ROOT, 'apps/desktop/messages/en-GB.json');
const BASELINE_PATH = path.join(here, 'i18n-catalog-baseline.txt');

// A duplicate value is noise (not flagged) unless it is multi-word or longer
// than this — short single-word duplicates (column headers, pane titles) are
// the catalog's intentional per-screen convention.
const DUP_MIN_LENGTH = 12;

/**
 * Zero entries means the catalog shape changed (nested groups instead of flat
 * string values), not that the catalog is clean: without this throw the lint
 * reported OK over a live violation and --generate drained the baseline.
 */
function loadCatalog() {
  const data = JSON.parse(readFileSync(CATALOG_PATH, 'utf8'));
  const entries = Object.entries(data).filter(
    ([key, value]) => key !== '$schema' && typeof value === 'string',
  );
  if (entries.length === 0) {
    throw new Error(
      `${CATALOG_PATH}: no string message values parsed — the catalog layout changed; update scripts/check-i18n-catalog.mjs.`,
    );
  }
  return entries;
}

function findCodeParamViolations(entries) {
  return entries
    .filter(([, value]) => value.includes('{code}'))
    .map(([key]) => key)
    .sort();
}

function findDuplicateValueGroups(entries) {
  const groups = new Map();
  for (const [key, value] of entries) {
    const trimmed = value.trim();
    const isNoise = !/\s/.test(trimmed) && trimmed.length <= DUP_MIN_LENGTH;
    if (isNoise) continue;
    const norm = trimmed.toLowerCase();
    if (!groups.has(norm)) groups.set(norm, []);
    groups.get(norm).push(key);
  }
  // Signature = sorted, comma-joined key list. Adding a NEW key to an
  // already-baselined group changes its signature (and therefore fails) —
  // deliberate: a new author choosing to duplicate existing wording is a new
  // violation even if the wording itself was already duplicated once.
  return [...groups.values()]
    .filter((keys) => keys.length >= 2)
    .map((keys) => [...keys].sort().join(','));
}

function loadBaseline() {
  let raw = '';
  try {
    raw = readFileSync(BASELINE_PATH, 'utf8');
  } catch {
    return { codeParam: new Set(), dupValue: new Set() };
  }
  const codeParam = new Set();
  const dupValue = new Set();
  for (const line of raw.split('\n')) {
    const l = line.trim();
    if (!l || l.startsWith('#')) continue;
    const tab = l.indexOf('\t');
    if (tab === -1) continue;
    const kind = l.slice(0, tab);
    const payload = l.slice(tab + 1);
    if (kind === 'code-param') codeParam.add(payload);
    else if (kind === 'dup-value') dupValue.add(payload);
  }
  return { codeParam, dupValue };
}

function writeBaseline(codeParam, dupValue) {
  const lines = [
    '# i18n catalog lint baseline — grandfathered violations.',
    '# scripts/check-i18n-catalog.mjs fails CI on any code-param / dup-value',
    '# violation NOT listed here. Entries drain via the i18n refactor sweep.',
    '# Regenerate with: node scripts/check-i18n-catalog.mjs --generate',
    ...codeParam.map((k) => `code-param\t${k}`),
    ...dupValue.map((s) => `dup-value\t${s}`),
    '',
  ];
  writeFileSync(BASELINE_PATH, lines.join('\n'));
}

function main() {
  const entries = loadCatalog();
  const codeParam = findCodeParamViolations(entries);
  const dupValue = findDuplicateValueGroups(entries);

  if (process.argv.includes('--generate')) {
    writeBaseline(codeParam, dupValue);
    console.log(
      `i18n-catalog-baseline.txt regenerated: ${codeParam.length} code-param, ${dupValue.length} dup-value.`,
    );
    return;
  }

  const baseline = loadBaseline();
  const newCodeParam = codeParam.filter((k) => !baseline.codeParam.has(k));
  const newDupValue = dupValue.filter((s) => !baseline.dupValue.has(s));

  if (newCodeParam.length === 0 && newDupValue.length === 0) {
    console.log(
      `i18n catalog lint: OK (${baseline.codeParam.size} code-param, ${baseline.dupValue.size} dup-value baselined).`,
    );
    return;
  }

  console.error('i18n catalog lint FAILED:\n');
  if (newCodeParam.length > 0) {
    console.error(
      'New message(s) interpolate the raw "{code}" parameter — map the error through the i18n error-code registry instead of showing it raw:',
    );
    for (const k of newCodeParam) console.error(`  - ${k}`);
    console.error('');
  }
  if (newDupValue.length > 0) {
    console.error(
      'New message(s) duplicate an existing catalog value exactly — reuse the existing key instead of hardcoding the same wording again:',
    );
    for (const s of newDupValue) console.error(`  - ${s}`);
    console.error('');
  }
  console.error(
    'If a violation is genuinely intentional, add it to scripts/i18n-catalog-baseline.txt (or run --generate) and note why in the PR.',
  );
  process.exitCode = 1;
}

main();
