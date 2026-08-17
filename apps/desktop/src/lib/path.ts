// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Shared path utilities used across the inbox, sessions, and calibration
 * features. These helpers handle cross-platform path joining and segmentation
 * where the backend guarantees forward-slash-normalized relative paths while
 * the OS root may use backslashes (Windows).
 */

/**
 * Resolve a reveal target path: the OS-native source root joined with a
 * forward-slash-normalized relative path. Handles Windows (backslash) roots
 * transparently — all separators in the joined path are rewritten to match the
 * root's native separator so the OS-native reveal command receives a valid
 * path.
 *
 * Backend contract: `relativePath` is ALWAYS forward-slash, even on Windows
 * (`crates/app/inbox/src/scan.rs` `replace('\\', "/")`), while the root
 * (`source.path`) is native (backslash on Windows, from the folder picker).
 * `pathe.join` is unsuitable here because it rewrites backslashes to forward
 * slashes, which the Windows select-item shell call rejects.
 *
 * This is the canonical implementation. It was previously duplicated as
 * `resolveRevealPath` in `features/sessions/revealInventory.ts` and
 * `resolveInboxRevealPath` in `features/inbox/inboxDetailHelpers.ts` (both
 * had identical bodies; the inbox copy accepted only `string`, this one
 * accepts the wider `string | null | undefined` union that the sessions copy
 * already used).
 */
export function resolveRevealPath(
  rootPath: string,
  relativePath: string | null | undefined,
): string {
  if (!relativePath) return rootPath;
  const sep = rootPath.includes('\\') ? '\\' : '/';
  const root = rootPath.replace(/[/\\]+$/, '');
  const rel = relativePath.replace(/^[/\\]+/, '').replace(/[/\\]+/g, sep);
  return `${root}${sep}${rel}`;
}

/**
 * Return the last path segment (filename or directory name) of a
 * forward-slash or back-slash-separated path.
 *
 * Previously duplicated in `features/inbox/inboxDetailHelpers.ts` and
 * `features/inbox/planPanelHelpers.ts` (both had identical bodies).
 *
 * Note: `InboxControls.tsx` has a distinct null-safe/trailing-slash-trim
 * variant (`basename(p: string | null | undefined): string | null`) that
 * keeps its own copy intentionally.
 */
export function basename(path: string): string {
  const parts = path.replace(/\\/g, '/').split('/');
  return parts[parts.length - 1] || path;
}

/**
 * Return the second-to-last path segment (the basename's parent directory
 * name). Returns an empty string when the path has fewer than two segments.
 *
 * Previously only in `features/inbox/inboxDetailHelpers.ts`.
 */
export function parentSegment(path: string): string {
  const parts = path.replace(/\\/g, '/').split('/').filter(Boolean);
  return parts.length >= 2 ? parts[parts.length - 2] : '';
}
