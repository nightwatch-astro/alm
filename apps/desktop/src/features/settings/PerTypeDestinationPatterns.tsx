// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Self-contained editor for the `patternsByType` naming-scope key (spec 041
 * T051, FR-026b). It loads and saves directly (rather than via the parent
 * `save` debounce) so it can surface the backend `value.invalid` rejection
 * inline — the parent auto-save swallows write errors.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { Btn } from '@/ui';
import { m } from '@/lib/i18n';
import { errMessage } from '@/lib/errors';
import { getSettings, patternPathPreview, updateSettings } from './settingsIpc';
import { SettingsSection, SettingsRow } from './SettingsKit';
import {
  chipsAreEmpty,
  emptyChipsByClass,
  FRAME_TYPE_CLASSES,
  FRAME_TYPE_DEFAULT_PATTERNS,
  frameTypeLabel,
  type FrameTypeClass,
  type PathChip,
  parsePathPattern,
  PER_TYPE_SAMPLE_METADATA,
  serializePathPattern,
  validatePatternString,
} from './naming-model';
import { PerTypePatternChipsEditor } from './PerTypePatternChipsEditor';

const SAVE_DEBOUNCE_MS = 300;

export function PerTypeDestinationPatterns() {
  // Override map: class → chip list. Empty array = built-in default.
  const [chipsByClass, setChipsByClass] =
    useState<Record<FrameTypeClass, PathChip[]>>(emptyChipsByClass);
  const [backendErrors, setBackendErrors] = useState<
    Partial<Record<FrameTypeClass, string>>
  >({});
  const [loaded, setLoaded] = useState(false);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Per-class live preview, resolved by the real backend `pattern.path_preview`
  // command (package P11) — keyed by class, absent while loading/unavailable.
  const [previewsByClass, setPreviewsByClass] = useState<
    Partial<Record<FrameTypeClass, { path: string; missingTokens: string[] }>>
  >({});
  const [previewErrorsByClass, setPreviewErrorsByClass] = useState<
    Partial<Record<FrameTypeClass, string>>
  >({});

  // ── Load saved overrides on mount ────────────────────────────────────────
  useEffect(() => {
    getSettings({ scope: 'naming' })
      .then((data) => {
        const vals = data.values as Record<string, unknown>;
        const raw = vals.patternsByType;
        if (raw && typeof raw === 'object') {
          const next = emptyChipsByClass();
          for (const cls of FRAME_TYPE_CLASSES) {
            const v = (raw as Record<string, unknown>)[cls];
            if (typeof v === 'string' && v.trim() !== '') {
              next[cls] = parsePathPattern(v);
            }
          }
          setChipsByClass(next);
        }
      })
      .catch(() => {
        // Use defaults on load failure (e.g. in test/mock environment).
      })
      .finally(() => setLoaded(true));
  }, []);

  // ── Per-class live preview (package P11) ─────────────────────────────────
  //
  // Resolves the effective pattern (override or built-in default) for every
  // class against representative sample metadata via the real resolver. Runs
  // whenever chips or backend validation errors change, after the initial
  // load completes. A class with a client- or backend-detected error is
  // skipped (no preview shown, mirroring the previous stub's behaviour).
  useEffect(() => {
    if (!loaded) return;
    let cancelled = false;

    void (async () => {
      const nextPreviews: Partial<
        Record<FrameTypeClass, { path: string; missingTokens: string[] }>
      > = {};
      const nextErrors: Partial<Record<FrameTypeClass, string>> = {};

      await Promise.all(
        FRAME_TYPE_CLASSES.map(async (cls) => {
          const chips = chipsByClass[cls];
          const isOverridden = !chipsAreEmpty(chips);
          const patternStr = isOverridden ? serializePathPattern(chips) : '';
          const clientError = isOverridden
            ? validatePatternString(patternStr)
            : null;
          const error = backendErrors[cls] ?? clientError ?? undefined;
          if (error != null) return; // No preview while the pattern is invalid.

          const effectivePattern = isOverridden
            ? patternStr
            : FRAME_TYPE_DEFAULT_PATTERNS[cls];
          try {
            const resp = await patternPathPreview(
              effectivePattern,
              PER_TYPE_SAMPLE_METADATA,
            );
            nextPreviews[cls] = {
              path: resp.resolvedPath,
              missingTokens: resp.missingTokens,
            };
          } catch (err: unknown) {
            // `errMessage` resolves a ContractError code (pattern.empty,
            // token.unknown, path.traversal, …) to its translated catalog
            // entry; anything unrecognisable gets the safe generic fallback.
            nextErrors[cls] = errMessage(err);
          }
        }),
      );

      if (!cancelled) {
        setPreviewsByClass(nextPreviews);
        setPreviewErrorsByClass(nextErrors);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [chipsByClass, backendErrors, loaded]);

  // ── Persist the full override map (debounced, captures backend errors) ────
  const persist = useCallback((next: Record<FrameTypeClass, PathChip[]>) => {
    if (saveTimer.current) clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => {
      // Send only non-empty overrides; an empty/absent class means "default".
      const payload: Record<string, string> = {};
      for (const cls of FRAME_TYPE_CLASSES) {
        if (!chipsAreEmpty(next[cls])) {
          payload[cls] = serializePathPattern(next[cls]);
        }
      }
      void updateSettings({
        scope: 'naming',
        values: { patternsByType: payload },
      }).then(
        () => {
          // Clear any stale backend errors on a successful save.
          setBackendErrors({});
        },
        (err: unknown) => {
          // Backend rejected at least one pattern (error code value.invalid).
          // We cannot tell which class from a single payload; flag all classes
          // that currently fail client-side validation, falling back to a
          // banner keyed on the first overridden class. `errMessage` resolves
          // the ContractError code to its translated catalog entry.
          const message = errMessage(err);
          const errs: Partial<Record<FrameTypeClass, string>> = {};
          let attributed = false;
          for (const cls of FRAME_TYPE_CLASSES) {
            if (!chipsAreEmpty(next[cls])) {
              const patStr = serializePathPattern(next[cls]);
              if (validatePatternString(patStr) !== null) {
                errs[cls] = message;
                attributed = true;
              }
            }
          }
          if (!attributed) {
            const firstOverride = FRAME_TYPE_CLASSES.find(
              (cls) => !chipsAreEmpty(next[cls]),
            );
            if (firstOverride) errs[firstOverride] = message;
          }
          setBackendErrors(errs);
        },
      );
    }, SAVE_DEBOUNCE_MS);
  }, []);

  const handleChipsChange = (cls: FrameTypeClass, chips: PathChip[]) => {
    const next = { ...chipsByClass, [cls]: chips };
    setChipsByClass(next);
    // Clear this class's backend error optimistically; re-validated on save.
    setBackendErrors((prev) => {
      if (!(cls in prev)) return prev;
      const { [cls]: _removed, ...rest } = prev;
      return rest;
    });
    persist(next);
  };

  const handleReset = (cls: FrameTypeClass) => {
    const next = { ...chipsByClass, [cls]: [] };
    setChipsByClass(next);
    setBackendErrors((prev) => {
      if (!(cls in prev)) return prev;
      const { [cls]: _removed, ...rest } = prev;
      return rest;
    });
    persist(next);
  };

  return (
    <SettingsSection title={m.settings_naming_pertype_title()}>
      {FRAME_TYPE_CLASSES.map((cls) => {
        const chips = chipsByClass[cls];
        const isOverridden = !chipsAreEmpty(chips);
        const patternStr = isOverridden ? serializePathPattern(chips) : '';
        const clientError =
          loaded && isOverridden ? validatePatternString(patternStr) : null;
        const error = backendErrors[cls] ?? clientError ?? undefined;
        const rowId = `naming-pattern-${cls}`;
        // Live preview: resolved by the real backend `pattern.path_preview`
        // command (package P11) against representative sample metadata. Only
        // shown when the pattern is free of validation errors.
        const preview = error == null ? previewsByClass[cls] : undefined;
        const previewUnavailable = error == null && previewErrorsByClass[cls];
        return (
          <SettingsRow
            key={cls}
            label={<span id={`${rowId}-label`}>{frameTypeLabel(cls)}</span>}
            info={m.settings_naming_dest_info()}
          >
            {/* Editor and its buttons live on separate lines (spec 043 §4). */}
            <div className="pv-naming__pertype-stack">
              <div
                className="pv-naming__pertype-editor-wrap"
                role="group"
                aria-labelledby={`${rowId}-label`}
                data-testid={rowId}
              >
                <PerTypePatternChipsEditor
                  chips={chips}
                  onChange={(c) => handleChipsChange(cls, c)}
                  error={error}
                  defaultPlaceholder={FRAME_TYPE_DEFAULT_PATTERNS[cls]}
                  rowId={rowId}
                />
              </div>

              {/* Working live preview of the resolved sample path. Announced
								    politely so screen-reader users hear updates while editing. */}
              {preview && preview.path !== '' && (
                <div
                  className="pv-naming__pertype-preview"
                  aria-live="polite"
                  data-testid={`${rowId}-preview`}
                >
                  <span className="pv-naming__pertype-preview-label">
                    {m.settings_naming_preview_label()}
                  </span>{' '}
                  <code className="pv-mono pv-naming__pertype-preview-code">
                    {preview.path}
                  </code>
                  {preview.missingTokens.length > 0 && (
                    <span className="pv-naming__preview-fallback">
                      {m.settings_naming_fallback_used({
                        tokens: preview.missingTokens.join(', '),
                      })}
                    </span>
                  )}
                  {!isOverridden && (
                    <span className="pv-naming__pertype-preview-default">
                      {m.settings_naming_preview_default()}
                    </span>
                  )}
                </div>
              )}
              {previewUnavailable && (
                <div
                  className="pv-naming__preview-error"
                  role="alert"
                  data-testid={`${rowId}-preview-error`}
                >
                  {previewUnavailable}
                </div>
              )}

              {/* Buttons on their own line. */}
              <div className="pv-naming__pertype-actions">
                <Btn
                  size="sm"
                  disabled={!isOverridden}
                  data-testid={`naming-pattern-reset-${cls}`}
                  onClick={() => handleReset(cls)}
                >
                  {m.common_reset()}
                </Btn>
              </div>
            </div>
          </SettingsRow>
        );
      })}
    </SettingsSection>
  );
}
