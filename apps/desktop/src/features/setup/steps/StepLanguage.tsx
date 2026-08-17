// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// First-run wizard: "Language" step (spec 061 US1) — the wizard's first
// step, so a user who cannot read the base locale picks a language before any
// of the other steps (which are explained in prose).
//
// Reuses the ThemePicker swatch card styling that General.tsx's theme
// picker already established (bordered card, active state via
// border/box-shadow) instead of cloning a parallel CSS block — this step
// only needs the name label, not the theme-preview strip. Cards are plain
// `<button>` elements, so Tab reaches every option — including any below the
// fold once the shipped set grows — without a hand-rolled roving-tabindex,
// and the wizard's own scrollable body (`WizardShell`'s `.pv-wizard__scroll`)
// already handles overflow, so no dedicated scroll container is needed here.

import { clsx } from 'clsx';
import { useLocale, SHIPPED_LOCALES } from '@/data/locale';
import { LOCALE_META, needsReviewNotice } from '@/data/locale-meta';
import { m } from '@/lib/i18n';

export function StepLanguage() {
  const { locale, changeLocale } = useLocale();

  return (
    <div className="pv-step-language">
      <div className="pv-locale-choices">
        {SHIPPED_LOCALES.map((id) => {
          const meta = LOCALE_META[id];
          const isActive = locale === id;
          const reviewNoticeId = `setup-language-${id}-review-notice`;
          const showReviewNotice = needsReviewNotice(id);
          return (
            <button
              key={id}
              type="button"
              lang={meta.id}
              className={clsx(
                'pv-locale-choice',
                isActive && 'pv-locale-choice--active',
              )}
              onClick={() => changeLocale(id)}
              aria-pressed={isActive}
              aria-describedby={showReviewNotice ? reviewNoticeId : undefined}
              // Accessible name comes from the native name only, never the
              // flag (research D6) — a screen reader announcing "flag of
              // Brazil" would be noise on top of the visible label below.
              aria-label={meta.nativeName}
            >
              <span className="pv-locale-choice__name">
                <span aria-hidden="true">{meta.flag}</span> {meta.nativeName}
              </span>
              {showReviewNotice && (
                <span
                  id={reviewNoticeId}
                  className="pv-text-xs pv-text-muted"
                  lang={locale}
                >
                  {m.setup_language_review_notice()}
                </span>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}
