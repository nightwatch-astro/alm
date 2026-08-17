// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * NotesSection — observing notes editor/viewer for a target (spec 023 US4).
 *
 * Extracted from TargetDetail.tsx.
 */

import { Section, Banner } from '@/ui';
import { m } from '@/lib/i18n';

export interface NotesSectionProps {
  notes: string | null;
  editing: boolean;
  setEditing: (v: boolean) => void;
  draft: string;
  setDraft: (v: string) => void;
  saving: boolean;
  saved: boolean;
  setSaved: (v: boolean) => void;
  error: string | null;
  setError: (v: string | null) => void;
  onSave: () => void;
}

export function NotesSection({
  notes,
  editing,
  setEditing,
  draft,
  setDraft,
  saving,
  saved,
  setSaved,
  error,
  setError,
  onSave,
}: NotesSectionProps) {
  return (
    <Section title={m.targets_detail_notes_title()}>
      {editing ? (
        <div className="pv-target-detail__notes-edit">
          <textarea
            data-testid="target-notes-textarea"
            aria-label={m.targets_detail_notes_title()}
            className="pv-target-detail__notes-textarea"
            placeholder={m.targets_detail_notes_placeholder()}
            value={draft}
            rows={5}
            maxLength={16384}
            disabled={saving}
            onChange={(e) => {
              setDraft(e.target.value);
              setError(null);
            }}
          />
          <div className="pv-target-detail__notes-actions">
            <button
              className="pv-target-detail__action-btn pv-target-detail__action-btn--muted"
              disabled={saving}
              onClick={() => {
                setDraft(notes ?? '');
                setEditing(false);
                setError(null);
              }}
            >
              {m.common_cancel()}
            </button>
            <button
              className="pv-target-detail__action-btn"
              disabled={saving}
              onClick={() => void onSave()}
            >
              {saving ? m.common_saving() : m.common_save()}
            </button>
          </div>
          {error && (
            <Banner variant="danger" className="pv-target-detail__banner">
              {error}
            </Banner>
          )}
        </div>
      ) : (
        <div className="pv-target-detail__notes-view">
          {notes ? (
            <div
              data-testid="target-notes-body"
              className="pv-target-detail__notes-body"
            >
              {notes}
            </div>
          ) : (
            <span
              data-testid="target-notes-empty"
              className="pv-target-detail__notes-empty"
            >
              {m.targets_detail_notes_empty()}
            </span>
          )}
          <div className="pv-target-detail__notes-footer">
            <button
              className="pv-target-detail__edit-btn"
              onClick={() => {
                setDraft(notes ?? '');
                setEditing(true);
                setSaved(false);
              }}
            >
              {m.common_edit()}
            </button>
            {saved && (
              <span className="pv-target-detail__notes-saved">
                {m.targets_detail_notes_saved()}
              </span>
            )}
          </div>
        </div>
      )}
    </Section>
  );
}
