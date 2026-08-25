// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * ProjectNotesSection — spec 024 T4.2.
 *
 * Inline-edit section for project free-text notes. Features:
 * - Renders existing notes body or "No notes." placeholder.
 * - Edit affordance: click "Edit" to open a textarea with save/cancel.
 * - 5-second debounce before issuing `project.note.update` (A5).
 * - Byte counter with warning when approaching / exceeding 16 384-byte cap.
 * - `note.content_too_large` error mapped to inline validation message.
 * - `project.read_only` error surfaced as a toast (archived project).
 */

import { useState, useEffect } from 'react';
import { useDebouncedCallback } from 'use-debounce';
import { Btn } from '@/ui';
import { m } from '@/lib/i18n';
import { addToast } from '@/shared/toast';
import { noteByteStatus } from '@/lib/notes';
import { NoteByteCounter } from '@/components/NoteByteCounter';
import {
  saveNote,
  getProjectNote,
  noteByteLength,
  MAX_NOTE_BYTES,
  NOTE_DEBOUNCE_MS,
} from './manifests';

// ── Props ─────────────────────────────────────────────────────────────────────

export interface ProjectNotesSectionProps {
  projectId: string;
  /** Current notes content from the project detail (may be null/undefined). */
  initialContent?: string | null;
  /** Whether the project lifecycle prevents editing (archived). */
  readOnly?: boolean;
}

// ── Component ─────────────────────────────────────────────────────────────────

export function ProjectNotesSection({
  projectId,
  initialContent,
  readOnly = false,
}: ProjectNotesSectionProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(initialContent ?? '');
  const [saving, setSaving] = useState(false);
  const [fieldError, setFieldError] = useState<string | null>(null);
  const [lastSaved, setLastSaved] = useState<string | null>(null);

  // Sync draft with parent-provided content (when a caller passes initialContent).
  useEffect(() => {
    if (initialContent !== undefined && !editing) {
      setDraft(initialContent ?? '');
    }
  }, [initialContent, editing]);

  // Self-fetch the persisted spec-024 note on mount when the parent does NOT
  // supply `initialContent` (e.g. the project detail drawer). Without this the
  // saved note is invisible after the drawer is reopened (spec 024 SC-002 /
  // US2 acceptance #1). Callers that pass `initialContent` keep prop-driven
  // behavior and skip the fetch.
  useEffect(() => {
    if (initialContent !== undefined) return;
    let cancelled = false;
    void (async () => {
      try {
        const res = await getProjectNote({ projectId });
        if (!cancelled) setDraft((prev) => (prev ? prev : (res.content ?? '')));
      } catch {
        // Backend unavailable — leave the placeholder; saving still works.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [projectId, initialContent]);

  const { overLimit } = noteByteStatus(draft);

  // ── Debounced autosave ────────────────────────────────────────────────────

  // Debounced autosave. `useDebouncedCallback` cancels the pending save on
  // unmount and replaces it on each keystroke, preserving the prior
  // setTimeout/clearTimeout semantics at the same NOTE_DEBOUNCE_MS interval.
  const triggerSave = useDebouncedCallback((content: string) => {
    // Keep the debounced callback void-returning (matching the prior
    // `setTimeout(async …)`); the async work runs in a fire-and-forget IIFE.
    void (async () => {
      if (noteByteLength(content) > MAX_NOTE_BYTES) return;
      setSaving(true);
      const { updatedAt, error } = await saveNote(projectId, content);
      setSaving(false);
      if (error === 'note.content_too_large') {
        setFieldError(
          m.projects_notes_byte_limit_exceeded({
            max: MAX_NOTE_BYTES.toLocaleString(),
          }),
        );
      } else if (error === 'project.read_only') {
        addToast({
          message: m.projects_toast_archived_readonly(),
          variant: 'error',
        });
      } else if (error) {
        addToast({
          message: m.projects_toast_save_notes_failed({ error: String(error) }),
          variant: 'error',
        });
      } else if (updatedAt) {
        setLastSaved(updatedAt);
        setFieldError(null);
      }
    })();
  }, NOTE_DEBOUNCE_MS);

  const handleChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const value = e.target.value;
    setDraft(value);
    setFieldError(null);
    if (!readOnly) triggerSave(value);
  };

  const handleSave = async () => {
    if (overLimit) {
      setFieldError(
        m.projects_notes_byte_limit_exceeded({
          max: MAX_NOTE_BYTES.toLocaleString(),
        }),
      );
      return;
    }
    triggerSave.cancel();
    setSaving(true);
    const { updatedAt, error } = await saveNote(projectId, draft);
    setSaving(false);
    if (error === 'note.content_too_large') {
      setFieldError(
        m.projects_notes_byte_limit_exceeded({
          max: MAX_NOTE_BYTES.toLocaleString(),
        }),
      );
    } else if (error === 'project.read_only') {
      addToast({
        message: m.projects_toast_archived_readonly(),
        variant: 'error',
      });
      setEditing(false);
    } else if (error) {
      addToast({
        message: m.projects_toast_save_notes_failed({ error: String(error) }),
        variant: 'error',
      });
    } else if (updatedAt) {
      setLastSaved(updatedAt);
      setFieldError(null);
      setEditing(false);
    }
  };

  const handleCancel = () => {
    triggerSave.cancel();
    setDraft(initialContent ?? '');
    setFieldError(null);
    setEditing(false);
  };

  // ── Render ────────────────────────────────────────────────────────────────

  if (!editing) {
    return (
      <div className="pv-project-notes__root" data-testid="project-notes-root">
        {draft ? (
          <div data-testid="notes-body" className="pv-project-notes__body">
            {draft}
          </div>
        ) : (
          <span data-testid="notes-empty" className="pv-project-notes__empty">
            {m.projects_notes_empty()}
          </span>
        )}
        {!readOnly && (
          <div>
            <Btn size="sm" variant="ghost" onClick={() => setEditing(true)}>
              {m.common_edit()}
            </Btn>
          </div>
        )}
        {lastSaved && (
          <span
            data-testid="notes-saved-indicator"
            className="pv-project-notes__saved"
          >
            {m.projects_notes_saved()}
          </span>
        )}
      </div>
    );
  }

  return (
    <div className="pv-project-notes__root" data-testid="project-notes-root">
      <textarea
        data-testid="notes-textarea"
        className="pv-input pv-project-notes__textarea"
        aria-label={m.projects_notes_label()}
        value={draft}
        onChange={handleChange}
        rows={6}
        disabled={saving}
        aria-invalid={Boolean(fieldError || overLimit)}
        aria-describedby={fieldError ? 'notes-field-error' : undefined}
      />
      <div className="pv-project-notes__toolbar">
        <NoteByteCounter content={draft} testId="notes-byte-counter" />
        <div className="pv-project-notes__actions">
          <Btn
            size="sm"
            variant="ghost"
            onClick={handleCancel}
            disabled={saving}
          >
            {m.common_cancel()}
          </Btn>
          <Btn
            size="sm"
            variant="primary"
            onClick={handleSave}
            disabled={saving || overLimit}
          >
            {saving ? m.common_saving() : m.projects_edit_save_btn()}
          </Btn>
        </div>
      </div>
      {fieldError && (
        <span
          id="notes-field-error"
          role="alert"
          data-testid="notes-field-error"
          className="pv-field-error"
        >
          {fieldError}
        </span>
      )}
    </div>
  );
}
