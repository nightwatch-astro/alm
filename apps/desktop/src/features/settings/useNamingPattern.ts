// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/** State + handlers for the top-level Project Folder Pattern (spec 015/018). */
import { useCallback, useEffect, useState } from 'react';
import { useMountedRef } from '@/hooks/useMountedRef';
import { m } from '@/lib/i18n';
import {
  getSettingsTyped,
  parseSettingsValues,
  type PatternPart,
  type PatternPreviewResponse,
  patternPreview,
  patternValidate,
} from './settingsIpc';
import { NamingSettingsSchema, type NamingSettings } from './settingsSchemas';
import { DEFAULT_PATTERN, SAMPLE_METADATA } from './naming-model';

export function useNamingPattern(
  save: (scope: string, values: Record<string, unknown>) => void,
) {
  const [pattern, setPattern] = useState<PatternPart[]>(DEFAULT_PATTERN);
  const [autoApplyPattern, setAutoApplyPattern] = useState(true);
  const [preview, setPreview] = useState<PatternPreviewResponse | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [validateResult, setValidateResult] = useState<{
    valid: boolean;
    warnings: string[];
    errorCode?: string;
  } | null>(null);
  const [loaded, setLoaded] = useState(false);

  const mountedRef = useMountedRef();

  const applyTyped = (vals: NamingSettings) => {
    if (vals.pattern !== undefined && vals.pattern.length > 0) {
      setPattern(vals.pattern);
    }
    if (vals.autoApplyPattern !== undefined) {
      setAutoApplyPattern(vals.autoApplyPattern);
    }
  };

  // `RestoreDefaultsBtn.onRestored` hands back a raw value bag, so it needs the
  // same validation the mount-time read gets.
  const applyValues = (vals: Record<string, unknown>) => {
    applyTyped(parseSettingsValues(vals, NamingSettingsSchema));
  };

  // ── Load saved pattern on mount (spec 018 keys: pattern, autoApplyPattern) ─
  useEffect(() => {
    getSettingsTyped('naming', NamingSettingsSchema)
      .then((vals) => {
        if (mountedRef.current) applyTyped(vals);
      })
      .catch(() => {
        // Use defaults on load failure (e.g. in test/mock environment).
      })
      .finally(() => {
        if (mountedRef.current) setLoaded(true);
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Live validation (T1.3 / T1.4) ────────────────────────────────────────
  const runValidation = useCallback(
    (parts: PatternPart[]) => {
      patternValidate(parts)
        .then((resp) => {
          if (!mountedRef.current) return;
          setValidateResult({
            valid: resp.valid,
            warnings: resp.warnings,
            errorCode: resp.errorCode ?? undefined,
          });
        })
        .catch(() => {
          // Ignore validation errors in mock/offline environments.
        });
    },
    [mountedRef],
  );

  // ── Live preview (T2.2 / T3.11) ─────────────────────────────────────────
  const runPreview = useCallback((parts: PatternPart[]) => {
    if (parts.length === 0) {
      setPreview(null);
      setPreviewError(null);
      return;
    }
    patternPreview(parts, SAMPLE_METADATA)
      .then((resp) => {
        if (!mountedRef.current) return;
        setPreview(resp);
        setPreviewError(null);
      })
      .catch((err: unknown) => {
        if (!mountedRef.current) return;
        setPreview(null);
        setPreviewError(
          typeof err === 'string'
            ? err
            : m.settings_naming_preview_unavailable(),
        );
      });
  }, []);

  // Run both when pattern changes, after initial load.
  useEffect(() => {
    if (!loaded) return;
    runValidation(pattern);
    runPreview(pattern);
  }, [pattern, loaded, runValidation, runPreview]);

  // ── Handle pattern change ─────────────────────────────────────────────────
  const handlePatternChange = (parts: PatternPart[]) => {
    setPattern(parts);
    // Persist immediately (spec 018 keys — noisy, no audit).
    save('naming', { pattern: parts, autoApplyPattern });
  };

  const handleAutoApplyChange = (checked: boolean) => {
    setAutoApplyPattern(checked);
    save('naming', { pattern, autoApplyPattern: checked });
  };

  const isValid = validateResult?.valid !== false;
  const canSave = isValid && pattern.length > 0;

  return {
    pattern,
    autoApplyPattern,
    preview,
    previewError,
    validateResult,
    applyValues,
    handlePatternChange,
    handleAutoApplyChange,
    canSave,
  };
}
