// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Args-template renderer (spec 011 T001, R3).
//!
//! Renders a `&[ArgsToken]` against a substitution context, returning a
//! `Vec<String>` of concrete CLI arguments. Only the closed token vocabulary
//! `{folder}` and `{file}` is supported.
//!
//! Constitution III: this module only builds argv; it never spawns processes.

use crate::ArgsToken;

/// Context supplied to [`render`] for token substitution.
#[derive(Clone, Debug, Default)]
pub struct RenderContext<'a> {
    /// The resolved working folder path. Used for `ArgsToken::Folder`.
    pub folder: Option<&'a str>,
    /// An optional selected file path. Used for `ArgsToken::File`.
    pub file: Option<&'a str>,
}

/// Render a `&[ArgsToken]` against the given context.
///
/// Each `Literal` is passed through unchanged. `Folder` and `File` tokens are
/// replaced with the corresponding context field. If a token's field is `None`
/// in the context the argument is simply omitted (not an error).
///
/// A substituted `Folder`/`File` value must be an absolute path: a relative value
/// whose first segment begins with `-` is read as an option by the callee's own
/// argument parser. Neither Siril nor PixInsight documents a `--` end-of-options
/// token, so absoluteness is the enforceable form of the operand boundary.
///
/// # Errors
///
/// Returns a descriptive string when a substituted `Folder`/`File` value is not
/// an absolute path.
pub fn render(template: &[ArgsToken], ctx: &RenderContext<'_>) -> Result<Vec<String>, String> {
    let mut out = Vec::with_capacity(template.len());
    for token in template {
        match token {
            ArgsToken::Literal(s) => out.push(s.clone()),
            ArgsToken::Folder => {
                if let Some(f) = ctx.folder {
                    out.push(absolute_operand("{folder}", f)?);
                }
            }
            ArgsToken::File => {
                if let Some(f) = ctx.file {
                    out.push(absolute_operand("{file}", f)?);
                }
            }
        }
    }
    Ok(out)
}

fn absolute_operand(token: &str, value: &str) -> Result<String, String> {
    if std::path::Path::new(value).is_absolute() {
        Ok(value.to_owned())
    } else {
        Err(format!("args-template token '{token}' requires an absolute path; got '{value}' (R3)"))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_folder_token() {
        let tokens = &[ArgsToken::Folder];
        let ctx = RenderContext { folder: Some("/mnt/library/project"), file: None };
        assert_eq!(render(tokens, &ctx).unwrap(), vec!["/mnt/library/project"]);
    }

    #[test]
    fn render_file_token() {
        let tokens = &[ArgsToken::File];
        let ctx = RenderContext { folder: None, file: Some("/mnt/library/project/capture.fit") };
        assert_eq!(render(tokens, &ctx).unwrap(), vec!["/mnt/library/project/capture.fit"]);
    }

    #[test]
    fn render_literal_token() {
        let tokens = &[ArgsToken::Literal("--open".to_owned()), ArgsToken::Folder];
        let ctx = RenderContext { folder: Some("/a/b"), file: None };
        assert_eq!(render(tokens, &ctx).unwrap(), vec!["--open", "/a/b"]);
    }

    #[test]
    fn render_omits_missing_folder() {
        let tokens = &[ArgsToken::Folder];
        let ctx = RenderContext { folder: None, file: None };
        assert_eq!(render(tokens, &ctx).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn render_empty_template_returns_empty() {
        assert_eq!(render(&[], &RenderContext::default()).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn render_rejects_relative_folder_operand() {
        let tokens = &[ArgsToken::Folder];
        let ctx = RenderContext { folder: Some("-rf"), file: None };
        let err = render(tokens, &ctx).unwrap_err();
        assert!(err.contains("{folder}"), "err: {err}");
        assert!(err.contains("absolute"), "err: {err}");
    }

    #[test]
    fn render_rejects_relative_file_operand() {
        let tokens = &[ArgsToken::File];
        let ctx = RenderContext { folder: None, file: Some("capture.fit") };
        let err = render(tokens, &ctx).unwrap_err();
        assert!(err.contains("{file}"), "err: {err}");
    }

    #[test]
    fn render_siril_folder() {
        use crate::seed;
        let profile = seed::find("siril").unwrap();
        let ctx = RenderContext { folder: Some("/mnt/lib/siril_project"), file: None };
        let argv = render(&profile.args_template, &ctx).unwrap();
        assert_eq!(argv, vec!["/mnt/lib/siril_project"]);
    }

    #[test]
    fn render_pixinsight_bare_no_args() {
        // #778: PixInsight can't open a bare folder path -- launch bare (no
        // args) rather than pass a broken {folder} token.
        use crate::seed;
        let profile = seed::find("pixinsight").unwrap();
        let ctx = RenderContext { folder: Some("/mnt/lib/pi_project"), file: None };
        let argv = render(&profile.args_template, &ctx).unwrap();
        assert_eq!(argv, Vec::<String>::new());
    }
}
