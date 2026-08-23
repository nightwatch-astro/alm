#!/usr/bin/env bash
# DB boundary guard — keystone enforcement for the persistence-layer boundary.
#
# Invariant: ALL production SQL lives inside `crates/persistence/`. ZERO raw
# sqlx query/exec sites are permitted in production Rust code outside those crates.
# This script counts those sites and FAILS if any exist. The checked-in baseline
# (db-boundary-baseline.txt) is sealed EMPTY and MUST stay empty — it is a locked
# zero, not a tunable ratchet. Any new leak fails CI.
#
# History: this began as a shrink-only ratchet during the persistence-layer
# hardening effort. Once every app-layer query was drained into
# crates/persistence/ (run `db-boundary-zero`), the baseline was sealed at
# zero. New SQL must be added as a persistence/* repository method — never as an
# app-layer sqlx call.
#
# Why a script and not clippy `disallowed-methods`: clippy cannot path-scope a
# lint to "everywhere except crates/persistence/". clippy.toml here provides a
# coarse secondary signal only; this guard is the real boundary enforcement.
#
# "Production" = `*.rs` files, excluding:
#   - crates/persistence/**            (the sanctioned home for SQL)
#   - any path containing a `tests/` segment (integration tests)
#   - the example reference module (query_builder_example.rs)
#   - query sites inside an inline `#[cfg(test)]` item (unit-test modules and
#     test-only helpers are not production code). Only the item's own scope is
#     exempt — production code following it in the same file is still counted.
#   - entire files the compiler excludes from production builds, i.e. file-level
#     test modules (see is_test_only_file)
#
# Usage:
#   scripts/check-db-boundary.sh            # enforce zero (CI mode)
#   scripts/check-db-boundary.sh --generate # re-seal the empty baseline; refuses if any leak exists
#   scripts/check-db-boundary.sh --list     # print current per-file counts
#   scripts/check-db-boundary.sh --self-test # prove the detector still detects

set -euo pipefail

# Repo root = parent of this script's directory.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BASELINE="$SCRIPT_DIR/db-boundary-baseline.txt"

# Patterns that denote a raw sqlx query/exec site.
#
# The constructor names must be followed by a call, a macro bang, or a turbofish.
# A bare `sqlx::query` also appears as a tracing target string
# (`event.metadata().target() == "sqlx::query"` in crates/tools/perf-bench), which
# names an event and executes nothing; counting it reported three query sites in a
# file that has none. Requiring the suffix drops that string, because a `"` follows
# it, and keeps every constructor a call site can use.
#
# `[[:alnum:]_]*` between the stem and the suffix is what admits the rest of the
# sqlx constructor family: `query_with`, `query_as_with`, `query_scalar_with`,
# `query_file!`, `query_file_as!`, `query_file_scalar!`. A stem alternation alone
# matched `sqlx::query` but not `sqlx::query_with(`, so a production site written
# with a bound-argument or file-backed constructor and consumed through `.fetch()`
# counted zero and the sealed boundary admitted it.
PATTERN='(sqlx::query|query_as|query_scalar|query_file)[[:alnum:]_]*[[:space:]]*(\(|!|::<)|\.fetch(_(one|all|optional))?\(|\.execute\('

# True when the compiler excludes this ENTIRE file from production builds.
#
# Two ways a whole file can be test-only, both checked against the compiler's
# actual rule rather than the file's name — a production file called `tests.rs`
# is still counted, which is why this is not a name-based exemption:
#
#   1. The file declares the inner attribute `#![cfg(test)]` in its header
#      region (blank/comment/attribute lines only). Note this is NOT matched by
#      the inline `#[cfg(test)]` cutoff regex below: the `!` makes it an inner
#      attribute applying to the whole file, not a cutoff for what follows.
#   2. The file is a module whose parent declares it `#[cfg(test)] mod <name>;`.
#      Extracting an inline `#[cfg(test)] mod tests { .. }` into its own file
#      leaves the attribute on the parent's declaration, so the file itself
#      contains no cfg(test) marker at all.
is_test_only_file() {
  local file="$1"
  local first_item modname dir parent inner

  # (1) Inner attribute in the header region. Restricted to the leading run of
  # blank/comment/attribute lines so an `#![cfg(test)]` nested inside an inline
  # `mod foo { .. }` (legal, but scopes to that module only) does not count.
  first_item="$(grep -nvE '^[[:space:]]*(//.*)?$|^[[:space:]]*#!?\[' "$file" | head -1 | cut -d: -f1 || true)"
  inner="$(grep -nE '^[[:space:]]*#!\[cfg\(test\)\]' "$file" | head -1 | cut -d: -f1 || true)"
  if [[ -n "$inner" ]] && { [[ -z "$first_item" ]] || [[ "$inner" -lt "$first_item" ]]; }; then
    return 0
  fi

  # (2) Parent declares this module under #[cfg(test)].
  modname="$(basename "$file" .rs)"
  dir="$(dirname "$file")"
  if [[ "$modname" == "mod" ]]; then
    # `foo/mod.rs` is module `foo`, declared by foo's own parent directory.
    modname="$(basename "$dir")"
    dir="$(dirname "$dir")"
  elif [[ "$modname" == "lib" || "$modname" == "main" ]]; then
    return 1 # crate roots have no parent module
  fi
  for parent in "$dir/mod.rs" "$dir.rs" "$dir/lib.rs" "$dir/main.rs"; do
    [[ -f "$parent" ]] || continue
    # `#[cfg(test)]` on the line immediately preceding the `mod <name>;` decl.
    if grep -A1 -E '^[[:space:]]*#\[cfg\(test\)\][[:space:]]*$' "$parent" \
      | grep -qE "^[[:space:]]*(pub[[:space:]]*(\([^)]*\))?[[:space:]]+)?mod[[:space:]]+${modname}[[:space:]]*;"; then
      return 0
    fi
  done
  return 1
}

# Count production query sites in a single file.
#
# An inline `#[cfg(test)]` item exempts ITS OWN SCOPE only, not the rest of the
# file: production code may legally follow an inline test module, and SQL there
# is a real leak. The item's extent is found by its closing brace at the
# attribute's own indentation, which rustfmt guarantees. Brace *depth* counting
# is deliberately avoided because braces inside string literals (multi-line SQL,
# format strings) would desynchronise it; the indentation rule cannot be thrown
# off that way, and its only failure mode — a string line starting `}` at
# exactly that column — ends the exemption early, i.e. errs strict.
count_file() {
  local file="$1"

  if is_test_only_file "$file"; then
    echo 0
    return
  fi

  PAT="$PATTERN" awk '
    BEGIN { pat = ENVIRON["PAT"] }

    # Inside a #[cfg(test)] item: nothing counts until its closing brace.
    skipping {
      if ($0 ~ ("^" indent "\\}")) { skipping = 0 }
      next
    }

    # Line after a #[cfg(test)] attribute decides how far the exemption reaches.
    pending {
      if ($0 ~ /^[[:space:]]*#\[/) { next }        # stacked attributes
      pending = 0
      if ($0 ~ /\{/) { skipping = 1 }              # braced item: skip its scope
      next                                         # else `mod x;` — just this line
    }

    /^[[:space:]]*#\[cfg\(test\)\]/ {
      indent = $0
      sub(/#.*$/, "", indent)
      if ($0 ~ /\{/) { skipping = 1 } else { pending = 1 }
      next
    }

    # A line that is entirely a comment compiles to nothing, so prose quoting a
    # call form is not a call site. Only full-line comments are dropped: stripping
    # a trailing `//` would also cut a `//` inside a string literal and could hide
    # real code on the same line.
    /^[[:space:]]*\/\// { next }

    # Block comments compile to nothing either, and unlike `//` they span lines, so
    # a full-line rule cannot see them. `/* sqlx::query("SELECT 1") */` counted as a
    # site and pushed the baseline up with no production query behind it.
    #
    # Removing the span rather than skipping the line is what makes
    # `sqlx::query /* runtime */ ("SELECT 1")` count: the pattern allows only
    # whitespace between the constructor and its `(`, and deleting the comment
    # leaves exactly that. Nesting is not handled, because Rust nests block
    # comments and awk has no counter here; a nested comment ends the span early and
    # leaves the tail to be matched, which over-counts rather than under-counts, and
    # a ratchet that fails loudly beats one that passes quietly.
    {
      line = $0
      if (in_block) {
        idx = index(line, "*/")
        if (idx == 0) { next }
        line = substr(line, idx + 2)
        in_block = 0
      }
      while ((s = index(line, "/*")) > 0) {
        rest = substr(line, s + 2)
        e = index(rest, "*/")
        if (e == 0) { line = substr(line, 1, s - 1); in_block = 1; break }
        line = substr(line, 1, s - 1) substr(rest, e + 2)
      }
      $0 = line
    }

    $0 ~ pat { n++ }
    END { print n + 0 }
  ' "$file"
}

# The source roots, resolved against $SCOPE_ROOT so the self-test below can point
# the same enumeration at an empty tree.
SCOPE_ROOT="$ROOT"
SCOPE_DIRS=(crates apps)

# Files the last require_scoped_files call enumerated; printed by the OK line so
# the reported zero is tied to a scope that was proven non-empty.
SCANNED_FILES=0

# Enumerate candidate production files (sorted, repo-relative paths).
# Callers MUST run require_scoped_files first: find's errors are not suppressed
# here, so an unguarded call on a collapsed tree fails loudly.
list_files() {
  cd "$SCOPE_ROOT"
  # Search source roots; prune crates/persistence/ and any tests/ directory.
  find "${SCOPE_DIRS[@]}" -type f -name '*.rs' \
    -not -path 'crates/persistence/*' \
    -not -path '*/tests/*' \
    -not -name 'query_builder_example.rs' \
    | sort
}

# Refuse a scope that enumerates nothing.
#
# The boundary is sealed at zero, so an empty enumeration is indistinguishable
# from a clean run: every file counts 0 and the gate prints OK. The floor is per
# root, not on the total, because one root emptying leaves the other still
# counting and a total-only floor would stay satisfied.
#
# list_files runs in a subshell at every call site (`$(...)`, `< <(...)`), where
# its exit status is discarded even under `set -o pipefail`, so the floor has to
# be asserted by the caller.
require_scoped_files() {
  local failed=0 d n files
  SCANNED_FILES=0

  for d in "${SCOPE_DIRS[@]}"; do
    if [[ ! -d "$SCOPE_ROOT/$d" ]]; then
      echo "FATAL: source root missing: $d" >&2
      failed=1
    fi
  done

  if [[ "$failed" -eq 0 ]]; then
    files="$(list_files)"
    for d in "${SCOPE_DIRS[@]}"; do
      n="$(printf '%s\n' "$files" | grep -c "^$d/" || true)"
      if [[ "$n" -eq 0 ]]; then
        echo "FATAL: source root enumerated 0 production .rs files: $d" >&2
        failed=1
      fi
    done
    SCANNED_FILES="$(printf '%s\n' "$files" | grep -c . || true)"
  fi

  if [[ "$failed" -ne 0 ]]; then
    echo "The source tree moved or the walk is broken; this gate cannot report a sealed boundary until that is fixed." >&2
    return 1
  fi
}

# Prove the detector still detects before any run reports zero.
#
# A sealed-at-zero guard fails open: if the pattern stops matching, every file
# counts 0 and the run prints OK. That is exactly how the pattern was narrowed
# from a bare `sqlx::query` — the narrowing is only safe because these cases run
# first. Each case is a synthetic file counted through count_file, so the fixture
# exercises the awk scanner, the cfg(test) cutoff, and the pattern together.
self_test() {
  local tmp expected actual name body entry status=0
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' RETURN

  # name|expected count|body (\n escapes expanded by printf %b)
  local -a cases=(
    'plain_query|1|let r = sqlx::query("SELECT 1").execute(p).await;'
    'turbofish|1|let r = sqlx::query_as::<_, Row>("SELECT 1");'
    'macro_bang|1|let r = sqlx::query_scalar!("SELECT 1");'
    'query_with|1|let r = sqlx::query_with("SELECT 1", args).execute(p).await;'
    'query_as_with|1|let r = sqlx::query_as_with::<_, Row, _>("SELECT 1", args);'
    'query_file_macro|1|let r = sqlx::query_file!("queries/one.sql");'
    'query_file_as_macro|1|let r = sqlx::query_file_as!(Row, "queries/one.sql");'
    'fetch_one|1|let r = builder.fetch_one(p).await;'
    'fetch_stream|1|let mut s = builder.fetch(p);'
    'block_comment_inline|0|/* let r = sqlx::query("SELECT 1"); */'
    'block_comment_span|0|/*\nlet r = sqlx::query("SELECT 1");\n*/'
    'comment_between_stem_and_call|1|let r = sqlx::query /* runtime */ ("SELECT 1");'
    'code_after_block_comment_closes|1|/* note */ let r = sqlx::query("SELECT 1");'
    'tracing_target|0|if event.metadata().target() == "sqlx::query" { n += 1; }'
    'doc_mention|0|/// Wraps sqlx::query_as::<_, Row>() for callers.'
    'cfg_test_scope|0|#[cfg(test)]\nmod tests {\n    fn t() { sqlx::query("SELECT 1"); }\n}'
    'after_cfg_test|1|#[cfg(test)]\nmod tests {\n    fn t() {}\n}\nfn prod() { sqlx::query("SELECT 1"); }'
  )

  for entry in "${cases[@]}"; do
    name="${entry%%|*}"
    body="${entry##*|}"
    expected="${entry#*|}"
    expected="${expected%%|*}"
    printf '%b\n' "$body" > "$tmp/$name.rs"
    actual="$(count_file "$tmp/$name.rs")"
    if [[ "$actual" != "$expected" ]]; then
      echo "FAIL: self-test case '$name' counted $actual, expected $expected." >&2
      status=1
    fi
  done

  # The detector cases above prove the pattern; these prove the enumeration that
  # feeds it, which is the other half of the sealed-at-zero failure mode.
  mkdir -p "$tmp/roots/${SCOPE_DIRS[0]}" "$tmp/roots/${SCOPE_DIRS[1]}"
  if (SCOPE_ROOT="$tmp/roots" require_scoped_files) >/dev/null 2>&1; then
    echo "FAIL: self-test: require_scoped_files accepted source roots holding 0 .rs files." >&2
    status=1
  fi

  rm -rf "$tmp/roots/${SCOPE_DIRS[0]}"
  if (SCOPE_ROOT="$tmp/roots" require_scoped_files) >/dev/null 2>&1; then
    echo "FAIL: self-test: require_scoped_files accepted a missing source root." >&2
    status=1
  fi

  if [[ "$status" -ne 0 ]]; then
    echo "db-boundary self-test: FAIL" >&2
    return 1
  fi
  echo "db-boundary self-test: PASS (detector flags call sites, not target strings or prose; an empty scope is refused)."
}

# Emit "count<TAB>path" for every file that has >=1 production query site.
collect() {
  local f n
  while IFS= read -r f; do
    n="$(count_file "$ROOT/$f")"
    if [[ "$n" -gt 0 ]]; then
      printf '%d\t%s\n' "$n" "$f"
    fi
  done < <(list_files)
}

case "${1:-}" in
  --generate)
    require_scoped_files || exit 2
    # Re-seal the baseline. The boundary is locked at ZERO, so this refuses to
    # bake in any leakage: if production query sites exist, drain them into
    # crates/persistence/db instead of recording a non-empty baseline.
    total="$(collect | awk -F'\t' '{s+=$1} END{print s+0}')"
    if [[ "$total" -ne 0 ]]; then
      echo "ERROR: refusing to generate a non-empty baseline ($total production query site(s) found)." >&2
      echo "The DB boundary is sealed at zero. Move these queries into crates/persistence/:" >&2
      collect >&2
      exit 1
    fi
    {
      echo "# DB boundary baseline — production sqlx query/exec sites OUTSIDE crates/persistence/."
      echo "# SEALED AT ZERO: this file must contain no count rows. All production SQL lives in"
      echo "# crates/persistence/; new queries are added there as repository methods, never here."
      echo "# Generated by scripts/check-db-boundary.sh --generate (refuses to record any leakage)."
    } > "$BASELINE"
    echo "Sealed baseline: $BASELINE"
    echo "  files scanned: $SCANNED_FILES   total production query sites: 0"
    ;;

  --list)
    require_scoped_files || exit 2
    collect
    ;;

  --self-test)
    self_test
    ;;

  ""|--check)
    self_test >/dev/null || {
      echo "FAIL: db-boundary detector self-test failed; a zero result is not trustworthy." >&2
      exit 1
    }
    require_scoped_files || exit 2

    if [[ ! -f "$BASELINE" ]]; then
      echo "ERROR: baseline missing: $BASELINE" >&2
      echo "Run: scripts/check-db-boundary.sh --generate" >&2
      exit 2
    fi

    # The boundary is sealed at zero. The baseline must contain no count rows,
    # and there must be no production query sites outside crates/persistence/db.
    fail=0

    # (1) Guard the seal itself: a non-empty baseline would silently re-open the
    # boundary, so reject any count row hand-edited back in.
    if grep -vE '^[[:space:]]*#' "$BASELINE" | grep -qE '[^[:space:]]'; then
      echo "SEAL BROKEN: $BASELINE contains count rows; the baseline must stay empty (zero-tolerance)." >&2
      fail=1
    fi

    # (2) Enforce zero production query sites.
    while IFS=$'\t' read -r cnt path; do
      echo "BOUNDARY VIOLATION: $path has $cnt production query site(s); zero allowed outside crates/persistence/." >&2
      fail=1
    done < <(collect)

    if [[ "$fail" -ne 0 ]]; then
      echo "" >&2
      echo "DB boundary guard failed: raw SQL is only allowed inside crates/persistence/." >&2
      echo "Add the query as a persistence/* repository method instead of an app-layer sqlx call." >&2
      exit 1
    fi

    echo "DB boundary OK — 0 production query site(s) across $SCANNED_FILES scanned file(s) outside crates/persistence/ (sealed at zero)."
    ;;

  *)
    echo "usage: $0 [--check|--generate|--list]" >&2
    exit 2
    ;;
esac
