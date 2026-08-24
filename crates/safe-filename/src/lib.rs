// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Security-conscious sanitization of a single path segment / filename
//! component.
//!
//! Turns an arbitrary string (e.g. a metadata value destined for a folder or
//! file name) into one that is safe to place in a cross-platform path, or
//! rejects it with a typed [`SanitizeError`]. The pipeline is stricter than a
//! plain character filter on the trojan-source / homoglyph axis: it strips
//! bidi overrides and Unicode format characters and rejects mixed-script
//! confusables.
//!
//! Steps (applied in order):
//! 1. NFC normalization + strip C0/C1 controls, format chars, bidi overrides.
//! 2. OS character substitution: Windows reserved chars and `/` → `_`, trim
//!    leading/trailing whitespace and dots.
//! 3. Path-traversal rejection: `.` or `..`.
//! 4. Windows reserved device-name rejection (CON, PRN, AUX, NUL, COM1–9,
//!    LPT1–9) on the pre-first-dot stem, case-insensitive, on all platforms.
//! 5. Unicode confusables detection via `unicode-security` (mixed-script),
//!    relaxed to admit a mixed Latin/Greek value.
//!
//! A successful [`sanitize_token_value`] returns exactly one usable path
//! segment: the result never contains a separator and is never empty.
//!
//! Each step is exposed individually so a caller can run them in sequence and
//! surface the first hard error, or call [`sanitize_token_value`] for the full
//! pipeline. `token_name` parameters are a free-text label used only for error
//! context (this crate is domain-agnostic; it originated as the token-value
//! sanitizer in a filesystem path-pattern resolver).

use unicode_normalization::UnicodeNormalization;
use unicode_script::{Script, UnicodeScript};
use unicode_security::MixedScript;

// ── Sanitize errors ────────────────────────────────────────────────────────

/// Errors that the sanitize pipeline can surface.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SanitizeError {
    /// Token value equals `.` or `..`, or the assembled path contains `..`.
    #[error("path traversal attempt in segment: {segment}")]
    PathTraversal { segment: String },
    /// Path segment matches a Windows reserved device name.
    #[error("Windows reserved device name: {segment}")]
    ReservedName { segment: String },
    /// Token value contains Unicode confusables or disallowed characters.
    #[error("Unicode confusables or disallowed chars in token {token}: {value}")]
    UnicodeConfusable { token: String, value: String },
    /// Steps 1 and 2 together consumed the whole value, leaving nothing usable
    /// as a path segment.
    #[error("token {token} is empty after sanitization: {value}")]
    EmptyAfterSanitize { token: String, value: String },
}

// ── Step 1: NFC + strip disallowed code-points ─────────────────────────────

/// C0 controls: U+0000–U+001F (but space U+0020 is allowed in separators only;
/// for token values it is stripped here and re-added if the token value
/// contains a meaningful space — in practice raw FITS values do not).
///
/// C1 controls: U+0080–U+009F.
/// Format characters: Unicode category Cf (bidi overrides U+200B, U+FEFF, etc.).
fn is_disallowed(c: char) -> bool {
    let cp = c as u32;
    // C0 controls (except we keep printable ASCII range U+0020–U+007E)
    if cp < 0x20 {
        return true;
    }
    // C1 controls
    if (0x80..=0x9F).contains(&cp) {
        return true;
    }
    // Unicode category Cf (format characters) — covers bidi overrides,
    // zero-width spaces, word joiners, etc.
    matches!(
        c,
        '\u{00AD}' // soft hyphen
        | '\u{200B}'..='\u{200F}' // zero-width sp, LRM, RLM, etc.
        | '\u{2028}'..='\u{202E}' // line/paragraph sep, bidi overrides
        | '\u{2060}'..='\u{206F}' // word joiner, inhibit symmetric, etc.
        | '\u{FE00}'..='\u{FE0F}' // variation selectors
        | '\u{FEFF}'              // BOM / zero-width no-break space
        | '\u{FFF9}'..='\u{FFFB}' // annotation chars
    )
}

/// Step 1: NFC-normalize the string and strip C0/C1 controls, format
/// characters, and bidi override code points.
///
/// Returns the cleaned string. Never errors.
#[must_use]
pub fn step1_normalize_and_strip(input: &str) -> String {
    // NFC normalization followed by disallowed-char removal.
    input.nfc().filter(|c| !is_disallowed(*c)).collect()
}

// ── Step 2: OS character substitution ──────────────────────────────────────

/// Characters that are illegal in Windows filenames / path segments.
/// `/` and `\` are path delimiters; `?`, `*`, `"`, `<`, `>`, `|`, `:` are
/// Windows-reserved. They are mapped to `_`.
fn is_windows_reserved_char(c: char) -> bool {
    matches!(c, '/' | '\\' | ':' | '?' | '*' | '"' | '<' | '>' | '|')
}

/// Step 2: Replace Windows-reserved characters with `_`, then trim leading/
/// trailing whitespace and dots.
///
/// `/` is substituted here, which is what makes a sanitized value exactly one
/// path segment rather than however many its content implies. Callers that
/// split a multi-segment pattern do so *before* sanitizing each piece, so a
/// literal directory level in a pattern is unaffected.
#[must_use]
pub fn step2_substitute_reserved_chars(input: &str) -> String {
    let substituted: String =
        input.chars().map(|c| if is_windows_reserved_char(c) { '_' } else { c }).collect();
    // Trim leading/trailing dots and whitespace (Windows disallows trailing dot
    // and leading/trailing spaces in path segment names).
    substituted.trim_matches(|c: char| c == '.' || c.is_whitespace()).to_owned()
}

// ── Step 3: Path-traversal check ───────────────────────────────────────────

/// Step 3: Return an error if the segment equals `.` or `..`.
///
/// The assembled path is also checked by the resolver for embedded `..`
/// segments (which can arise from unusual token values that survive step 2).
///
/// # Errors
/// Returns [`SanitizeError::PathTraversal`] when `segment` is `.` or `..`.
pub fn step3_traversal_check(segment: &str) -> Result<(), SanitizeError> {
    if segment == "." || segment == ".." {
        return Err(SanitizeError::PathTraversal { segment: segment.to_owned() });
    }
    Ok(())
}

// ── Step 4: Windows reserved device name check ─────────────────────────────

/// Windows device names that must not appear as path segment names.
///
/// Checked case-insensitively on all platforms (constitution requirement).
static WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Step 4: Return an error if the segment (after prior sanitization) names a
/// Windows reserved device, case-insensitively.
///
/// Windows resolves a device name from the text before the **first** dot, so
/// `CON.fits` and `NUL.xisf` name devices just as `CON` does. The comparison is
/// therefore on that stem, not on the whole segment.
///
/// # Errors
/// Returns [`SanitizeError::ReservedName`] when the segment is a reserved name.
pub fn step4_reserved_name_check(segment: &str) -> Result<(), SanitizeError> {
    let stem = segment.split('.').next().unwrap_or(segment);
    let upper = stem.to_uppercase();
    if WINDOWS_RESERVED_NAMES.contains(&upper.as_str()) {
        return Err(SanitizeError::ReservedName { segment: segment.to_owned() });
    }
    Ok(())
}

// ── Step 5: Unicode confusables check ──────────────────────────────────────

/// Scripts a value may mix freely: Latin and Greek, plus the script-neutral
/// `Common` and `Inherited` (digits, punctuation, combining marks).
///
/// Bayer designations attach a Greek letter to a Latin constellation name, so
/// `α Centauri` is a legitimate catalogue name that a plain mixed-script
/// rejection refuses. Admitting this one pair also admits a Latin/Greek
/// homoglyph (Greek ο against Latin o); no other script pair is admitted.
fn is_astronomy_script(c: char) -> bool {
    matches!(c.script(), Script::Latin | Script::Greek | Script::Common | Script::Inherited)
}

/// Step 5: Check the sanitized value for Unicode confusables using the
/// `unicode-security` crate's mixed-script confusable detection, relaxed to
/// admit a value mixing only Latin and Greek.
///
/// # Errors
/// Returns [`SanitizeError::UnicodeConfusable`] when the value mixes scripts
/// beyond the Latin/Greek pair.
pub fn step5_confusables_check(token_name: &str, value: &str) -> Result<(), SanitizeError> {
    // The unicode-security crate checks whether a string is "safe" from the
    // mixed-script confusables perspective (Unicode TR #39 §4).
    // `MixedScript::is_single_script` is implemented on `&str`.
    if !value.is_ascii() && !value.is_single_script() && !value.chars().all(is_astronomy_script) {
        return Err(SanitizeError::UnicodeConfusable {
            token: token_name.to_owned(),
            value: value.to_owned(),
        });
    }
    Ok(())
}

// ── Convenience: full pipeline ─────────────────────────────────────────────

/// Run the full sanitize pipeline on a token value.
///
/// Returns exactly one usable path segment on success: never empty, never
/// containing a separator. Returns the first hard error encountered.
///
/// # Errors
/// Returns a [`SanitizeError`] if any step rejects the value, including
/// [`SanitizeError::EmptyAfterSanitize`] when the steps consume it entirely.
/// Reporting that as an error rather than `Ok("")` is what keeps the emptiness
/// check off every caller.
pub fn sanitize_token_value(token_name: &str, raw: &str) -> Result<String, SanitizeError> {
    let s = step1_normalize_and_strip(raw);
    // Step 3 (traversal check) must run BEFORE step 2 so that `.` / `..` are
    // caught before step 2's dot-trimming collapses them to an empty string.
    step3_traversal_check(&s)?;
    let s = step2_substitute_reserved_chars(&s);
    if s.is_empty() {
        return Err(SanitizeError::EmptyAfterSanitize {
            token: token_name.to_owned(),
            value: raw.to_owned(),
        });
    }
    step4_reserved_name_check(&s)?;
    step5_confusables_check(token_name, &s)?;
    Ok(s)
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rstest::rstest;

    // ── Step 1: normalize and strip ────────────────────────────────────────

    /// Table-driven step-1 cases. Each row asserts that `input` normalizes to
    /// `expected` after NFC + disallowed-char stripping.
    #[rstest]
    // \x00 (C0 control / NUL) is removed. Note `\x00` consumes exactly two hex
    // digits, so "M\x0031" is M + NUL + '3' + '1'; stripping NUL yields "M31".
    #[case("M\x0031", "M31")]
    // U+202E RIGHT-TO-LEFT OVERRIDE (bidi) is stripped.
    #[case("normal\u{202E}text", "normaltext")]
    // U+200B ZERO WIDTH SPACE is stripped.
    #[case("ha\u{200B}lpha", "halpha")]
    // Normal ASCII is preserved unchanged.
    #[case("NGC7000", "NGC7000")]
    // "é" as decomposed (U+0065 U+0301) is NFC-composed to U+00E9.
    #[case("e\u{0301}", "\u{00E9}")]
    fn step1_normalize_and_strip_cases(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(step1_normalize_and_strip(input), expected);
    }

    #[test]
    fn step1_strips_c0_controls_contains_check() {
        // Preserves the original assertion's explicit "no NUL survives" intent.
        let result = step1_normalize_and_strip("M\x0031");
        assert!(!result.contains('\x00'));
    }

    // ── Step 2: substitute reserved chars ──────────────────────────────────

    /// Table-driven step-2 cases covering reserved-char substitution and the
    /// leading/trailing dot+whitespace trim.
    #[rstest]
    #[case("C:drive", "C_drive")] // colon → _
    #[case("foo\\bar", "foo_bar")] // backslash → _
    #[case("what?", "what_")] // question mark → _
    #[case("glob*", "glob_")] // asterisk → _
    #[case(".hidden.", "hidden")] // leading/trailing dots trimmed
    #[case("trailing ", "trailing")] // trailing space trimmed
    #[case("NGC-7000_Ha", "NGC-7000_Ha")]
    // inner hyphen/underscore preserved
    // astro-plan-3v3r.20.24: `/` is substituted, so a value cannot add a level.
    #[case("Ha/OIII", "Ha_OIII")] // dual-narrowband FILTER header
    #[case("M42/Trapezium", "M42_Trapezium")] // OBJECT header with a slash
    #[case("a/b/c", "a_b_c")] // every separator, not just the first
    fn step2_substitute_reserved_chars_cases(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(step2_substitute_reserved_chars(input), expected);
    }

    /// astro-plan-3v3r.20.24: the pipeline's output is exactly one segment.
    #[test]
    fn sanitize_token_value_never_yields_a_separator() {
        let out = sanitize_token_value("filter", "Ha/OIII").unwrap();
        assert_eq!(out, "Ha_OIII");
        assert!(!out.contains('/'));
    }

    // ── Step 3: traversal check ────────────────────────────────────────────

    /// Table-driven step-3 cases: `.`/`..` are rejected, everything else passes.
    #[rstest]
    #[case("..", true)] // rejected
    #[case(".", true)] // rejected
    #[case("NGC7000", false)] // allowed
    fn step3_traversal_check_cases(#[case] segment: &str, #[case] expect_err: bool) {
        let result = step3_traversal_check(segment);
        if expect_err {
            assert!(matches!(result, Err(SanitizeError::PathTraversal { .. })));
        } else {
            assert!(result.is_ok());
        }
    }

    // ── Step 4: reserved name check ────────────────────────────────────────

    /// Table-driven step-4 cases: Windows device names are rejected
    /// case-insensitively; prefixes and normal names pass.
    #[rstest]
    #[case("CON", true)] // uppercase reserved
    #[case("nul", true)] // lowercase reserved
    #[case("COM9", true)] // numbered device
    #[case("lpt1", true)] // lowercase numbered device
    #[case("CONtrast", false)] // CON prefix is not reserved
    #[case("NGC7000", false)]
    // normal name
    // astro-plan-3v3r.1.22: Windows resolves a device name from the text before
    // the FIRST dot, so an extension does not make the segment a filename.
    #[case("CON.fits", true)] // device name with an extension
    #[case("nul.xisf", true)] // lowercase, with an extension
    #[case("COM1.json", true)] // numbered device with an extension
    #[case("CON.foo.json", true)] // `file_stem` would keep `CON.foo` and pass
    #[case("CONtrast.fits", false)] // a real filename whose stem is not a device
    #[case("NGC7000.fits", false)] // ordinary astronomy filename
    fn step4_reserved_name_check_cases(#[case] segment: &str, #[case] expect_err: bool) {
        let result = step4_reserved_name_check(segment);
        if expect_err {
            assert!(matches!(result, Err(SanitizeError::ReservedName { .. })));
        } else {
            assert!(result.is_ok());
        }
    }

    // ── Step 5: confusables check ──────────────────────────────────────────

    /// Table-driven step-5 cases: pure ASCII and single-script non-ASCII pass.
    #[rstest]
    #[case("NGC7000")] // pure ASCII
    #[case("Andromède")]
    // single-script Latin (accented)
    // astro-plan-3v3r.8.39: a Bayer designation attaches a Greek letter to a
    // Latin constellation name; refusing it refuses a real catalogue name.
    #[case("α Centauri")] // Greek U+03B1 + Latin — the recorded harness vector
    #[case("β Cygni")] // second Bayer name, Greek + Latin
    #[case("μ Cephei")] // Greek + Latin
    fn step5_confusables_check_allows(#[case] value: &str) {
        assert!(step5_confusables_check("target", value).is_ok());
    }

    /// astro-plan-3v3r.8.39, the other direction: relaxing the rule to the
    /// Latin/Greek pair must not admit a homoglyph spoof from another script.
    #[rstest]
    #[case("mаsters")] // Cyrillic а (U+0430) inside an otherwise Latin word
    #[case("Аndromeda")] // Cyrillic А (U+0410) leading a Latin word
    #[case("M42\u{0430}")] // Cyrillic а appended to an ASCII designation
    fn step5_confusables_check_rejects_cross_script_homoglyphs(#[case] value: &str) {
        assert!(matches!(
            step5_confusables_check("target", value),
            Err(SanitizeError::UnicodeConfusable { .. })
        ));
    }

    // ── Full pipeline ──────────────────────────────────────────────────────

    #[test]
    fn full_pipeline_clean_value() {
        let result = sanitize_token_value("target", "NGC7000").unwrap();
        assert_eq!(result, "NGC7000");
    }

    #[test]
    fn full_pipeline_strips_bidi_and_sanitizes() {
        // Bidi override is stripped, then the colon is replaced.
        let result = sanitize_token_value("target", "HA\u{202E}:filter").unwrap();
        assert_eq!(result, "HA_filter");
    }

    #[test]
    fn full_pipeline_traversal_rejected() {
        let err = sanitize_token_value("target", "..").unwrap_err();
        assert!(matches!(err, SanitizeError::PathTraversal { .. }));
    }

    #[test]
    fn full_pipeline_reserved_name_rejected() {
        let err = sanitize_token_value("target", "CON").unwrap_err();
        assert!(matches!(err, SanitizeError::ReservedName { .. }));
    }

    /// astro-plan-3v3r.1.23: the pipeline reports "nothing left" as an error, so
    /// the emptiness check does not fall to every caller.
    #[rstest]
    #[case("...")] // dot-only
    #[case("  ..  ")] // padded traversal
    #[case("   ")] // whitespace-only
    #[case(". . .")] // dots and spaces, trimmed from both ends
    #[case("\u{200B}")] // a value that is only a stripped format character
    fn full_pipeline_empty_after_sanitize_is_an_error(#[case] raw: &str) {
        assert!(matches!(
            sanitize_token_value("target", raw),
            Err(SanitizeError::EmptyAfterSanitize { .. })
        ));
    }

    /// astro-plan-3v3r.1.22 through the whole pipeline, not only step 4.
    #[test]
    fn full_pipeline_reserved_name_with_extension_rejected() {
        let err = sanitize_token_value("target", "CON.fits").unwrap_err();
        assert!(matches!(err, SanitizeError::ReservedName { .. }));
    }

    /// astro-plan-3v3r.8.39 through the whole pipeline.
    #[test]
    fn full_pipeline_accepts_greek_bayer_name() {
        let out = sanitize_token_value("target", "α Centauri").unwrap();
        assert_eq!(out, "α Centauri");
    }

    // ── Property tests ─────────────────────────────────────────────────────
    //
    // Invariants over arbitrary input that the table-driven cases cannot cover
    // exhaustively. Cases are deterministic: proptest defaults to a fixed RNG
    // seed unless `PROPTEST_RNG_SEED` is set, so failures reproduce reliably.

    proptest! {
        // step1: never panics and never leaves a disallowed code point behind.
        #[test]
        fn step1_strips_all_disallowed(s in ".*") {
            let out = step1_normalize_and_strip(&s);
            prop_assert!(out.chars().all(|c| !is_disallowed(c)));
        }

        // step1 is idempotent: applying it twice equals applying it once.
        // (NFC is idempotent, and the second pass has nothing left to strip.)
        #[test]
        fn step1_is_idempotent(s in ".*") {
            let once = step1_normalize_and_strip(&s);
            let twice = step1_normalize_and_strip(&once);
            prop_assert_eq!(once, twice);
        }

        // step2: never panics; output contains no Windows-reserved chars and is
        // free of leading/trailing dots or whitespace.
        #[test]
        fn step2_removes_reserved_chars_and_trims(s in ".*") {
            let out = step2_substitute_reserved_chars(&s);
            prop_assert!(out.chars().all(|c| !is_windows_reserved_char(c)));
            prop_assert!(!out.starts_with('.') && !out.ends_with('.'));
            if let Some(first) = out.chars().next() {
                prop_assert!(!first.is_whitespace());
            }
            if let Some(last) = out.chars().last() {
                prop_assert!(!last.is_whitespace());
            }
        }

        // step2 is idempotent: a second pass changes nothing.
        #[test]
        fn step2_is_idempotent(s in ".*") {
            let once = step2_substitute_reserved_chars(&s);
            let twice = step2_substitute_reserved_chars(&once);
            prop_assert_eq!(once, twice);
        }

        // The full pipeline never panics on arbitrary input; on success the
        // returned value carries no reserved chars and is not a bare `.`/`..`.
        #[test]
        fn sanitize_token_value_never_panics(s in ".*") {
            // `Err(_)` (a hard rejection) is an acceptable, non-panicking
            // outcome; only the success case carries invariants to check.
            if let Ok(out) = sanitize_token_value("target", &s) {
                prop_assert!(out.chars().all(|c| !is_windows_reserved_char(c)));
                prop_assert!(out != "." && out != "..");
                // The one-segment contract: a success is never empty and never
                // spans a separator, for every input.
                prop_assert!(!out.is_empty());
                prop_assert!(!out.contains('/'));
            }
        }

        // Pure-ASCII alphanumeric segments that are not reserved names and not
        // traversal tokens always sanitize successfully and round-trip
        // unchanged (no reserved char, dot, or whitespace can appear).
        #[test]
        fn sanitize_token_value_roundtrips_clean_ascii(
            s in "[A-Za-z0-9][A-Za-z0-9_-]{0,30}"
        ) {
            prop_assume!(step4_reserved_name_check(&s).is_ok());
            let out = sanitize_token_value("target", &s).expect("clean ASCII must sanitize");
            prop_assert_eq!(out, s);
        }
    }
}
