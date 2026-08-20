#!/usr/bin/env bash
# Dead-caller guard — fails when a module-level `pub fn` in crates/ has no
# production caller, i.e. the only code that reaches it is its own tests.
#
# The defect class: a function is implemented, unit-tested, reviewed, and
# merged, but nothing in the shipped call graph ever invokes it. Every test
# passes, so CI is green and the feature is dead from the user's perspective.
# Issues #712 (write_session_snapshot / should_snapshot), #879
# (find_or_create_{camera,telescope}_by_alias) and the Rust half of #943
# (rank_candidates) are all this shape, and all reached main.
#
# Why a script and not a rustc/clippy lint: `dead_code` does not fire on a `pub`
# item in a lib crate, and `unreachable_pub` fires on the opposite condition —
# it flags items that CANNOT be reached from outside the crate and suggests
# narrowing them to `pub(crate)`. A `pub fn` in a `pub mod` with zero callers is
# externally reachable in principle, so rustc considers it live and stays
# silent. Verified: with `#![warn(unreachable_pub, dead_code, unused)]` all
# active, rustc emits nothing for the #712 shape. `--self-test` re-proves this
# script's own detection on every run so a green result is never vacuous.
#
# Detection is name-based, so it is deliberately narrow:
#   - Definitions come only from `crates/**/src/**/*.rs`, and only from
#     functions at column 0. Anything indented is inside an `impl` block, and
#     impl methods are legitimately reached through traits or receivers that a
#     name grep cannot see. Restricting to column 0 excludes them all.
#   - References come from production Rust: `crates/**` plus
#     `apps/desktop/src-tauri/src/**`, minus any `tests/` path segment, minus
#     each `#[cfg(test)]` item's own brace scope. Only that scope is exempt —
#     production code following an inline test module still counts, the same
#     rule check-db-boundary.sh applies. Comment lines are stripped, so a doc
#     comment naming a function does not disguise it as live.
#
# The baseline (dead-callers-baseline.txt) lists names already dead when the
# guard landed. It is a shrink-only ratchet: names may be removed as the debt is
# wired up or deleted, but a name that is not in the baseline fails the build.
#
# Usage:
#   scripts/check-dead-callers.sh             # enforce (CI mode)
#   scripts/check-dead-callers.sh --list      # print current dead names
#   scripts/check-dead-callers.sh --generate  # rewrite the baseline
#   scripts/check-dead-callers.sh --self-test # prove the detector still detects

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BASELINE="$SCRIPT_DIR/dead-callers-baseline.txt"

# Emit paths (one per line, in the same string form `find` would print them,
# since resolution starts from a path `find` already returned) of files that
# are out-of-line test modules: `#[cfg(test)] mod name;` in some other file,
# with the body living in a sibling `name.rs` or `name/` directory per Rust's
# module-file convention. Unlike `mod tests { ... }`, these files carry no
# #[cfg(test)] attribute of their own (it lives only on the declaration) and
# their directory need not be named `tests`, so neither the defs/corpus name
# filters nor the corpus brace-scope skip below can see them — they must be
# excluded by resolved path instead. A directory-form sibling is excluded
# wholesale (not just its mod.rs) so a further out-of-line submodule declared
# inside it (no attribute of its own needed — the cfg(test) gate is inherited
# from the parent) is swept up too.
#
# Assumes the attribute sits on its own line directly above the declaration
# (rustfmt's default, and the only shape found in this tree) — a same-line
# `#[cfg(test)] mod x;` is not matched.
#
# Cleans up $pairs directly rather than via `trap ... RETURN`: this function
# nests inside collect_dead (which sets its own RETURN trap), and bash RETURN
# traps are a single shell-wide slot, not a per-call stack — the inner trap
# would silently replace the outer one for the rest of the shell's life, the
# same pre-existing hazard that already applies to self_test calling
# collect_dead.
collect_test_mod_exclusions() {
    local pairs
    pairs="$(mktemp)"

    # shellcheck disable=SC2016  # the awk program is literal, not shell-expanded
    find "$@" -type f -name '*.rs' -print0 \
        | xargs -0 awk '
            # Crude cfg(test)-ish detector: tokenize on non-word chars and
            # look for both a `cfg` and a `test` token, so `#[cfg(test)]` and
            # `#[cfg(all(test, feature = "x"))]` both match while an unrelated
            # `#[cfg(target_os = "test")]`-shaped false positive stays rare
            # enough not to be worth a real parser.
            function is_test_attr(s,    t, n, i, toks, has_cfg, has_test) {
                t = s
                gsub(/[^A-Za-z0-9_]/, " ", t)
                n = split(t, toks, " ")
                for (i = 1; i <= n; i++) {
                    if (toks[i] == "cfg") has_cfg = 1
                    if (toks[i] == "test") has_test = 1
                }
                return (has_cfg && has_test)
            }
            function handle_item(    m) {
                if (item ~ /^[[:space:]]*(pub([[:space:]]*\([^)]*\))?[[:space:]]+)?mod[[:space:]]+[A-Za-z_][A-Za-z0-9_]*[[:space:]]*;[[:space:]]*$/) {
                    m = item
                    sub(/^[[:space:]]*(pub([[:space:]]*\([^)]*\))?[[:space:]]+)?mod[[:space:]]+/, "", m)
                    sub(/[[:space:]]*;.*/, "", m)
                    print FILENAME "\t" m
                }
                pending = 0
                item = ""
            }
            FNR == 1 { in_attr = 0; depth = 0; abuf = ""; pending = 0; item = "" }
            # accumulate a `#[cfg(\n ... \n)]`-shaped attribute split across lines
            in_attr {
                abuf = abuf " " $0
                o = gsub(/\[/, "["); c = gsub(/\]/, "]")
                depth += o - c
                if (depth <= 0) { in_attr = 0; if (is_test_attr(abuf)) pending = 1 }
                next
            }
            # blank/comment lines between the attribute and the item do not cancel it
            pending && /^[[:space:]]*$/ { next }
            pending && /^[[:space:]]*\/\// { next }
            # an attribute line: start (or stacked continuation of) an attribute list
            /^[[:space:]]*#\[/ {
                o = gsub(/\[/, "["); c = gsub(/\]/, "]")
                d = o - c
                if (d > 0) { in_attr = 1; depth = d; abuf = $0 }
                else if (is_test_attr($0)) { pending = 1 }
                next
            }
            # the item the attribute stack (if any) applies to
            {
                if (!pending) next
                item = $0
                handle_item()
            }
        ' > "$pairs"

    local declfile modname dir base subdir target_file target_dir
    while IFS="$(printf '\t')" read -r declfile modname; do
        [ -z "$declfile" ] && continue
        dir="$(dirname "$declfile")"
        base="$(basename "$declfile")"
        case "$base" in
            mod.rs | lib.rs | main.rs) subdir="$dir" ;;
            *) subdir="$dir/${base%.rs}" ;;
        esac
        target_file="$subdir/$modname.rs"
        target_dir="$subdir/$modname"
        [ -f "$target_file" ] && printf '%s\n' "$target_file"
        [ -d "$target_dir" ] && find "$target_dir" -type f -name '*.rs' -print
    done < "$pairs"
    rm -f "$pairs"
}

# find wrapper that inserts the out-of-line test-module exclusions computed by
# the caller ($exclude_args) before appending `-print0` itself — callers pass
# only their test predicates, never `-print0`. Both matter: `-not -path` must
# precede `-print0` in a find expression or it is evaluated too late to affect
# an action that already fired left-to-right; and bash 3.2 errors on
# `"${arr[@]}"` expansion of a truly empty array under `set -u`, so the
# emptiness check guards every call site instead of relying on 4.4+ behavior.
find_rs_files() {
    if [ "${#exclude_args[@]}" -eq 0 ]; then
        find "$@" -print0
    else
        find "$@" "${exclude_args[@]}" -print0
    fi
}

# Emit the sorted list of module-level `pub fn` names in $1 (a crates/ root)
# that no production line outside their own definition mentions.
collect_dead() {
    local crates_root="$1" tauri_root="$2"

    local defs corpus test_mod_files
    defs="$(mktemp)"
    corpus="$(mktemp)"
    test_mod_files="$(mktemp)"
    # shellcheck disable=SC2064  # expand paths now, not at trap time
    trap "rm -f '$defs' '$corpus' '$test_mod_files'" RETURN

    # Production corpus: comment lines dropped, then each `#[cfg(test)]` item
    # skipped for exactly its own brace scope. Truncating the whole file at the
    # first `#[cfg(test)]` instead would discard the production code that
    # follows an inline test module, which is common here and silently turns
    # live functions (pid_is_alive, discover_all) into false positives.
    #
    # `use` statements (`use x;`, `pub use x;`, `pub(crate) use x;`, grouped
    # `pub use x::{a, b};`, and their multi-line forms) are dropped for the
    # same reason `defs` excludes definition lines from counting as their own
    # caller: a re-export names a function without calling it. A module split
    # that turns a file into a `mod.rs` gains one `pub use` per moved item, and
    # without this exclusion every such name reads as "called" forever after,
    # silently disabling the guard for genuinely dead functions (#968 split of
    # inbox.rs hit this for find_orphaned_plan_links/set_manual_override; the
    # same shape pre-existed in repositories/projects/mod.rs and
    # repositories/first_run/mod.rs). Grouped imports span lines, so this
    # tracks brace depth exactly like the cfg(test) skip above.
    local -a roots=("$crates_root")
    [ -d "$tauri_root" ] && roots+=("$tauri_root")

    collect_test_mod_exclusions "${roots[@]}" > "$test_mod_files"
    local -a exclude_args=()
    local p
    while IFS= read -r p; do
        [ -n "$p" ] && exclude_args+=(-not -path "$p")
    done < "$test_mod_files"

    # Definitions exclude the e2e-tests crate (test code, not shipped), the
    # persistence query-builder reference module, which exists to be read rather
    # than called — check-db-boundary.sh exempts it for the same reason — and
    # any out-of-line test-module file resolved above: a `pub fn` test helper
    # in one of those is not a production definition either.
    find_rs_files "$crates_root" -type f -name '*.rs' \
        -not -path '*/tests/*' \
        -not -path '*/e2e-tests/*' \
        -not -name 'query_builder_example.rs' \
        | xargs -0 grep -hE '^pub (async )?fn [a-z_0-9]+' \
        | sed -E 's/^pub (async )?fn ([a-z_0-9]+).*/\2/' \
        | sort -u > "$defs"

    # shellcheck disable=SC2016  # the awk program is literal, not shell-expanded
    find_rs_files "${roots[@]}" -type f -name '*.rs' -not -path '*/tests/*' \
        | xargs -0 awk '
            FNR == 1 { skip = 0; depth = 0; opened = 0; inuse = 0; usedepth = 0 }
            /^[[:space:]]*\/\// { next }
            skip {
                o = gsub(/\{/, "{"); c = gsub(/\}/, "}")
                depth += o - c
                if (o > 0) opened = 1
                # An attribute on a brace-less item (`#[cfg(test)] use x;`)
                # scopes to that one line only.
                if (!opened && /;[[:space:]]*$/) { skip = 0; next }
                if (opened && depth <= 0) { skip = 0; opened = 0; depth = 0 }
                next
            }
            /#\[cfg\(test\)\]/ { skip = 1; depth = 0; opened = 0; next }
            inuse {
                o = gsub(/\{/, "{"); c = gsub(/\}/, "}")
                usedepth += o - c
                if (usedepth <= 0) { inuse = 0 }
                next
            }
            /^[[:space:]]*(pub([[:space:]]*\([^)]*\))?[[:space:]]+)?use[[:space:]]/ {
                o = gsub(/\{/, "{"); c = gsub(/\}/, "}")
                usedepth = o - c
                if (usedepth > 0) inuse = 1
                next
            }
            { print }
        ' > "$corpus"

    awk -v defs="$defs" '
        BEGIN {
            while ((getline name < defs) > 0) if (name != "") want[name] = 0
        }
        {
            line = $0
            # A definition line is not a call site. Suppress just that name.
            def = ""
            if (match(line, /^pub (async )?fn [a-z_0-9]+/)) {
                def = substr(line, RSTART, RLENGTH)
                sub(/^pub (async )?fn /, "", def)
            }
            gsub(/[^A-Za-z0-9_]/, " ", line)
            n = split(line, tok, " ")
            for (i = 1; i <= n; i++) {
                t = tok[i]
                if (t in want && t != def) want[t]++
            }
        }
        END { for (name in want) if (want[name] == 0) print name }
    ' "$corpus" | sort
}

# doc comment, one called only AFTER an inline test module (guards the
# brace-scope handling, since skipping to end-of-file instead would report it
# dead), two out-of-line `#[cfg(test)] mod name;` cases — a flat-file sibling
# and a `mod.rs`-directory sibling with its own nested submodule (guards
# collect_test_mod_exclusions, and the directory-form exclusion being swept
# recursively) — and a `mod.rs`-style split: a single-line `pub use ... as`, a
# multi-line grouped `pub use { ... };` re-exporting two otherwise-dead names,
# and a re-export of a genuinely live name — the last guards against the fix
# over-stripping and hiding a real caller. A detector that reports nothing, or
# that reports everything — the vacuous-green failure this guard exists to
# prevent — fails here.
# Require every NEWLY baselined name to carry a tracking reference in the
# comment block directly above it.
#
# Why: an entry here means "implemented, tested, and never called by shipped
# code". That is a debt item, but the file was being used as a suppression list
# -- 134 of 148 entries carried no bead or issue reference when this check was
# added, so nothing connected them back to work that would remove them. An
# untracked entry reads as settled, and a feature that never runs can then sit
# indefinitely while specs report it as shipped. That is exactly how
# `acquisition_fingerprint`'s missing writer disabled calibration matching for
# ~2.5 months (astro-plan-siyk, fixed in #1655) and the pattern recorded on
# astro-plan-68c7.
#
# Deliberately grandfathered: existing entries are NOT retro-required to carry a
# reference. Demanding 134 back-annotations at once would either block unrelated
# work or get satisfied with a copy-pasted placeholder, which is worse than
# silence because it looks like tracking. New entries only, so the file shrinks
# toward being fully tracked as it turns over.
#
# Accepts an `astro-plan-*` bead id or a `#NNNN` issue reference anywhere in the
# preceding comment block.
#
# One comment block covers the whole contiguous group of names beneath it, which
# is how the file is already written (see the groups at
# scripts/dead-callers-baseline.txt:32-48 and :71-96). A blank line ends a group;
# a comment line following a name starts a new block rather than extending the
# previous one.
classify_baseline() {
    awk '
        /^[[:space:]]*#/ {
            # A comment after a name opens a new block instead of appending to
            # the one that covered the group above it.
            if (after_name) { block = ""; after_name = 0 }
            block = block " " $0
            next
        }
        /^[[:space:]]*$/ { block = ""; after_name = 0; next }
        {
            after_name = 1
            print (block ~ /astro-plan-[a-z0-9.]+|#[0-9]+/ ? "T" : "U") "\t" $0
        }
    '
}

# Names in $2 (current baseline file) that violate the tracking rule, relative to
# $1 (base-revision baseline file). Prints `new<TAB>name` or `regressed<TAB>name`.
tracking_violations() {
    local base="$1" current="$2"
    local base_class current_class
    base_class="$(classify_baseline <"$base")"
    current_class="$(classify_baseline <"$current")"

    local current_untracked
    current_untracked="$(printf '%s\n' "$current_class" | awk -F'\t' '$1 == "U" { print $2 }' | sort -u)"
    [ -n "$current_untracked" ] || return 0

    # (1) Names this revision ADDS must carry a reference. Pre-existing untracked
    # names stay grandfathered.
    comm -13 \
        <(printf '%s\n' "$base_class" | cut -f2 | sort -u) \
        <(printf '%s\n' "$current_untracked") \
        | sed 's/^/new\t/'

    # (2) A name that HAD a reference in the base and lost it is a regression:
    # without this the ratchet holds only until the entry lands, after which the
    # comment can be deleted while the name stays put.
    comm -12 \
        <(printf '%s\n' "$base_class" | awk -F'\t' '$1 == "T" { print $2 }' | sort -u) \
        <(printf '%s\n' "$current_untracked") \
        | sed 's/^/regressed\t/'
}

# Revision to compare the baseline against, echoed on stdout. Empty means the
# base is unavailable.
#
# Why this fetches: the PR and merge-queue job uses the default shallow
# `actions/checkout@v4` (.github/workflows/ci.yml:280), which populates the event
# ref only -- `origin/main` does not exist there, so a bare `git show
# origin/main:...` fails and the gate skipped itself on every PR. On the
# post-merge main canary (:866) `origin/main` IS present but points at the commit
# just pushed, so the baseline would be compared with itself and nothing ever
# reads as added; the HEAD^ fallback below makes that run check the commit's own
# additions instead.
resolve_base_commit() {
    local ref="${DEAD_CALLERS_BASE_REF:-origin/main}"
    if ! git -C "$ROOT" rev-parse --verify --quiet "$ref^{commit}" >/dev/null 2>&1; then
        git -C "$ROOT" fetch --quiet --depth=1 origin \
            "+refs/heads/${ref#origin/}:refs/remotes/origin/${ref#origin/}" >/dev/null 2>&1 || true
        git -C "$ROOT" rev-parse --verify --quiet "$ref^{commit}" >/dev/null 2>&1 || return 0
    fi

    # A shallow fetch can leave no common ancestor, so fall back to the ref tip.
    local base
    base="$(git -C "$ROOT" merge-base HEAD "$ref" 2>/dev/null \
        || git -C "$ROOT" rev-parse "$ref^{commit}")"

    if [ "$base" = "$(git -C "$ROOT" rev-parse HEAD)" ]; then
        git -C "$ROOT" rev-parse --verify --quiet 'HEAD~1^{commit}' >/dev/null 2>&1 || return 0
        base="$(git -C "$ROOT" rev-parse 'HEAD~1')"
    fi
    printf '%s' "$base"
}

check_tracking_references() {
    local base_commit baseline_rel
    base_commit="$(resolve_base_commit)"
    if [ -z "$base_commit" ]; then
        # Failing closed in CI: a skip here is what let the check pass on every
        # PR while reading as enforced.
        if [ -n "${CI:-}" ]; then
            echo "FAIL: cannot resolve a base revision for the tracking check (tried ${DEAD_CALLERS_BASE_REF:-origin/main})." >&2
            echo "Fetch the base branch in the workflow, or set DEAD_CALLERS_BASE_REF." >&2
            return 1
        fi
        echo "NOTE: no base revision available locally; skipping the tracking check."
        return 0
    fi

    # `git show` needs a repo-relative path; $BASELINE is absolute.
    baseline_rel="$(git -C "$SCRIPT_DIR" rev-parse --show-prefix 2>/dev/null)dead-callers-baseline.txt"

    local base_file violations
    base_file="$(mktemp)"
    # shellcheck disable=SC2064  # expand now, not at trap time
    trap "rm -f '$base_file'" RETURN
    if ! git -C "$ROOT" show "$base_commit:$baseline_rel" >"$base_file" 2>/dev/null; then
        echo "NOTE: no baseline file at $base_commit; skipping the tracking check."
        return 0
    fi

    violations="$(tracking_violations "$base_file" "$BASELINE")"
    [ -n "$violations" ] || return 0

    echo "FAIL: baseline entries without a tracking reference (base $base_commit):" >&2
    printf '%s\n' "$violations" \
        | sed -e 's/^new\t/  added:     /' -e 's/^regressed\t/  lost ref:  /' >&2
    cat >&2 <<'EOF'

An entry here means the function is implemented and tested but never called by
shipped code. Add a comment directly above it naming the bead or issue that will
remove it -- an `astro-plan-*` id or `#NNNN`. One comment block covers the whole
group of names under it, so joining an existing group is enough.

`lost ref` means the entry HAD a reference and no longer does. Restore it rather
than leaving the name behind, or the tracking holds only until the entry lands.

Without a reference the entry reads as settled rather than owed, and a feature
that never runs can sit here indefinitely while the specs call it shipped. If the
function is genuinely not needed, delete it instead of baselining it.
EOF
    return 1
}

# Prove the tracking rule on fixture baselines where the answer is known.
#
# Needed because the real baseline passes the check, which is indistinguishable
# from a check that classifies nothing: every way this can break is silent. The
# group-carrying and lost-reference cases in particular each shipped as a bug in
# the first version of this block.
tracking_self_test() {
    local tmp failures=0
    tmp="$(mktemp -d)"
    # shellcheck disable=SC2064  # expand $tmp now, not at trap time
    trap "rm -rf '$tmp'" RETURN

    expect() {  # expect <label> <expected> <actual>
        [ "$2" = "$3" ] && return 0
        echo "FAIL: tracking self-test: $1" >&2
        echo "  expected: [$2]" >&2
        echo "  actual:   [$3]" >&2
        failures=$((failures + 1))
    }

    # A comment block covers every name in its group; a blank line ends the group;
    # a comment after a name opens a new block.
    cat >"$tmp/classify" <<'FIX'
# tracked by astro-plan-abcd
group_first
group_second

untracked_after_blank
# no reference in this block
new_block_name
FIX
    expect 'group carrying + blank/new-block resets' \
        'T group_first|T group_second|U untracked_after_blank|U new_block_name' \
        "$(classify_baseline <"$tmp/classify" | awk -F'\t' '{ printf "%s%s %s", sep, $1, $2; sep = "|" }')"

    # A `#NNNN` issue reference is accepted alongside a bead id.
    printf '# see #1655\nissue_tracked\n' >"$tmp/issue"
    expect 'issue reference is accepted' 'T issue_tracked' \
        "$(classify_baseline <"$tmp/issue" | tr '\t' ' ')"

    printf '# tracked by astro-plan-aaaa\nkept_tracked\n\nold_untracked\n' >"$tmp/base"

    # Grandfathered: an untracked name already in the base is not reported.
    expect 'pre-existing untracked name is grandfathered' '' \
        "$(tracking_violations "$tmp/base" "$tmp/base")"

    # A new name with no reference is reported; one sharing a tracked group is not.
    printf '# tracked by astro-plan-aaaa\nkept_tracked\nadded_in_group\n\nold_untracked\nadded_bare\n' >"$tmp/added"
    expect 'new untracked name reported, group-mate accepted' 'new	added_bare' \
        "$(tracking_violations "$tmp/base" "$tmp/added")"

    # Deleting the comment from a landed entry is a regression, not a pass.
    printf 'kept_tracked\n\nold_untracked\n' >"$tmp/regressed"
    expect 'a landed entry losing its reference is reported' 'regressed	kept_tracked' \
        "$(tracking_violations "$tmp/base" "$tmp/regressed")"

    if [ "$failures" -gt 0 ]; then
        echo "FAIL: tracking self-test: $failures case(s) failed." >&2
        return 1
    fi
    echo "OK: tracking self-test passed — group blocks carry, grandfathering holds, lost references fail."
}

self_test() {
    local tmp
    tmp="$(mktemp -d)"
    # shellcheck disable=SC2064  # expand $tmp now, not at trap time
    trap "rm -rf '$tmp'" RETURN
    mkdir -p "$tmp/crates/probe/src/outofline_dir_tests"

    cat > "$tmp/crates/probe/src/lib.rs" <<'PROBE'
pub fn live_fn() -> bool { true }

//! doc_only_fn is named here and nowhere else.
pub fn doc_only_fn() -> bool { true }

pub fn test_only_fn() -> bool { true }

pub fn caller() -> bool { live_fn() }

pub fn live_after_tests() -> bool { true }

pub fn reexport_dead_fn() -> bool { true }

pub use self::reexport_dead_fn as reexport_dead_alias;

pub fn grouped_reexport_dead_fn() -> bool { true }
pub fn grouped_reexport_dead_fn_2() -> bool { true }

pub use self::{
    grouped_reexport_dead_fn, grouped_reexport_dead_fn_2,
};

pub fn reexported_live_fn() -> bool { true }

pub use self::reexported_live_fn;

pub fn calls_reexported_live() -> bool { reexported_live_fn() }

pub fn outofline_flat_dead_fn() -> bool { true }

pub fn outofline_dir_dead_fn() -> bool { true }

pub fn outofline_nested_dead_fn() -> bool { true }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn t() { assert!(test_only_fn()); }
}

pub fn late_caller() -> bool { live_after_tests() }

#[cfg(test)]
mod outofline_flat_tests;

#[cfg(test)]
mod outofline_dir_tests;
PROBE

    # Flat-file sibling: `outofline_flat_tests.rs` next to lib.rs.
    cat > "$tmp/crates/probe/src/outofline_flat_tests.rs" <<'PROBE'
use super::*;

#[test]
fn flat_calls_dead() {
    assert!(outofline_flat_dead_fn());
}
PROBE

    # Directory sibling: `outofline_dir_tests/mod.rs`, itself declaring a
    # further out-of-line submodule with no attribute of its own (the
    # cfg(test) gate is inherited from the parent declaration in lib.rs).
    cat > "$tmp/crates/probe/src/outofline_dir_tests/mod.rs" <<'PROBE'
use super::*;

mod nested;

#[test]
fn dir_calls_dead() {
    assert!(outofline_dir_dead_fn());
}
PROBE

    cat > "$tmp/crates/probe/src/outofline_dir_tests/nested.rs" <<'PROBE'
use crate::*;

#[test]
fn nested_calls_dead() {
    assert!(outofline_nested_dead_fn());
}
PROBE

    local got expected
    got="$(collect_dead "$tmp/crates" "$tmp/none" | tr '\n' ' ')"
    expected="caller calls_reexported_live doc_only_fn grouped_reexport_dead_fn grouped_reexport_dead_fn_2 late_caller outofline_dir_dead_fn outofline_flat_dead_fn outofline_nested_dead_fn reexport_dead_fn test_only_fn "

    if [ "$got" != "$expected" ]; then
        echo "FAIL: self-test detector mismatch." >&2
        echo "  expected: $expected" >&2
        echo "  actual:   $got" >&2
        return 1
    fi
    echo "OK: self-test passed — detector flags test-only and doc-only functions, not called ones."
}

main() {
    case "${1:-}" in
        --self-test)
            self_test
            tracking_self_test
            return
            ;;
    esac

    local dead
    dead="$(collect_dead "$ROOT/crates" "$ROOT/apps/desktop/src-tauri/src")"

    case "${1:-}" in
        --list)
            printf '%s\n' "$dead"
            return
            ;;
        --generate)
            {
                cat <<'HDR'
# Module-level `pub fn`s in crates/ that no production code calls — only their
# own tests reach them. Generated by scripts/check-dead-callers.sh --generate.
#
# Shrink-only. Removing a name (by wiring the function into its call path or
# deleting it) is always fine. Adding one requires a comment explaining why the
# function is legitimately unreferenced.
HDR
                printf '%s\n' "$dead"
            } > "$BASELINE"
            echo "Wrote $(printf '%s\n' "$dead" | grep -c . || true) names to $BASELINE"
            return
            ;;
    esac

    self_test >/dev/null || {
        echo "FAIL: dead-caller detector self-test failed; results are not trustworthy." >&2
        exit 1
    }

    local baselined
    baselined="$(grep -vE '^\s*(#|$)' "$BASELINE" | sort)"

    local new
    new="$(comm -23 <(printf '%s\n' "$dead") <(printf '%s\n' "$baselined"))"

    if [ -n "$new" ]; then
        echo "FAIL: these pub fns have no production caller — only tests reach them:" >&2
        printf '%s\n' "$new" | sed 's/^/  /' >&2
        cat >&2 <<'EOF'

Each named function is implemented and tested but never called by shipped code,
so it is dead from the user's perspective while CI stays green.

Fix by wiring the function into its production call path. If it is genuinely
not needed, delete it. If it is intentionally unreferenced (a trait-object-only
entry point, a re-exported public API), add it to
scripts/dead-callers-baseline.txt with a comment saying why.
EOF
        exit 1
    fi

    local stale
    stale="$(comm -13 <(printf '%s\n' "$dead") <(printf '%s\n' "$baselined") || true)"
    if [ -n "$stale" ]; then
        echo "NOTE: baseline entries now have callers (or were deleted); drop them from $BASELINE:" >&2
        printf '%s\n' "$stale" | sed 's/^/  /' >&2
    fi

    check_tracking_references || exit 1

    echo "OK: no new dead pub fns ($(printf '%s\n' "$dead" | grep -c . || true) baselined)."
}

main "$@"
