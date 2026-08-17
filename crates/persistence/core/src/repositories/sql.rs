// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! SQLite query-building helpers shared by persistence crates.

/// Escape SQLite `LIKE` metacharacters (`%`, `_`, and the escape character
/// `\`) in user-supplied search text so literal strings match literally.
///
/// Pairs with `ESCAPE '\'` on the `LIKE` clause:
/// ```sql
/// WHERE LOWER(name) LIKE ? ESCAPE '\'
/// ```
///
/// Without escaping, an astronomical target name like `M31_L` would match any
/// name of the same length due to `_` acting as a single-char wildcard.
///
/// # Example
///
/// ```
/// use persistence_core::repositories::sql::escape_like;
/// assert_eq!(escape_like("M31_L"), "M31\\_L");
/// assert_eq!(escape_like("50%"), "50\\%");
/// assert_eq!(escape_like("a\\b"), "a\\\\b");
/// ```
#[must_use]
pub fn escape_like(input: &str) -> String {
    input.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// Wrap `query` in LIKE wildcards with metacharacters escaped.
///
/// Equivalent to `format!("%{}%", escape_like(query))` but saves the
/// allocation at every call site.  Pairs with `ESCAPE '\'` in the SQL clause.
#[must_use]
pub fn like_contains(query: &str) -> String {
    format!("%{}%", escape_like(query))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_like_leaves_plain_text_unchanged() {
        assert_eq!(escape_like("ngc 7000"), "ngc 7000");
    }

    #[test]
    fn escape_like_escapes_percent() {
        assert_eq!(escape_like("50%"), "50\\%");
    }

    #[test]
    fn escape_like_escapes_underscore() {
        assert_eq!(escape_like("M31_L"), "M31\\_L");
    }

    #[test]
    fn escape_like_escapes_backslash() {
        assert_eq!(escape_like("a\\b"), "a\\\\b");
    }

    #[test]
    fn like_contains_wraps_and_escapes() {
        assert_eq!(like_contains("M_31%"), "%M\\_31\\%%");
    }
}
