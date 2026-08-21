// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/// <reference types="@testing-library/jest-dom" />
/**
 * ProjectNotesSection tests — spec 024 T4.2.
 *
 * Tests:
 * 1. Renders "No notes." placeholder when no content.
 * 2. Renders existing notes body.
 * 3. Shows "Edit" button when not read-only.
 * 4. Hides "Edit" button when readOnly=true.
 * 5. Opens textarea on Edit click.
 * 6. Cancel restores original content and hides textarea.
 * 7. Save button calls saveNote and closes editing.
 * 8. Content too large shows field error.
 * 9. Byte counter reflects current content size.
 * 11-15. Debounced autosave (NOTE_DEBOUNCE_MS): timing, coalescing, and the
 *    three error mappings reachable only through the debounced path.
 */

import {
  render,
  screen,
  fireEvent,
  waitFor,
  act,
} from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// ── Mocks ─────────────────────────────────────────────────────────────────────

const { mockSaveNote, mockGetProjectNote, mockAddToast } = vi.hoisted(() => ({
  mockSaveNote: vi.fn(),
  mockGetProjectNote: vi.fn(),
  mockAddToast: vi.fn(),
}));

vi.mock('./manifests', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./manifests')>();
  return {
    ...actual,
    saveNote: mockSaveNote,
    getProjectNote: mockGetProjectNote,
  };
});

vi.mock('@/shared/toast', () => ({
  addToast: mockAddToast,
  useToasts: () => ({ toasts: [], dismiss: vi.fn(), add: vi.fn() }),
}));

// ── Import under test ─────────────────────────────────────────────────────────

import { ProjectNotesSection } from './ProjectNotesSection';
import { MAX_NOTE_BYTES, NOTE_DEBOUNCE_MS } from './manifests';
import { m } from '@/lib/i18n';

// ── Helpers ───────────────────────────────────────────────────────────────────

function renderNotes(
  props: Partial<React.ComponentProps<typeof ProjectNotesSection>> & {
    projectId?: string;
  } = {},
) {
  return render(
    <ProjectNotesSection
      projectId={props.projectId ?? 'proj-test'}
      initialContent={props.initialContent}
      readOnly={props.readOnly}
    />,
  );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('ProjectNotesSection', () => {
  beforeEach(() => {
    mockSaveNote.mockReset();
    mockAddToast.mockReset();
    mockGetProjectNote.mockReset();
    mockGetProjectNote.mockResolvedValue({
      projectId: 'proj-test',
      content: null,
    });
  });

  it('10. self-fetches the persisted note when no initialContent is provided (SC-002)', async () => {
    mockGetProjectNote.mockResolvedValue({
      projectId: 'proj-test',
      content: 'Persisted on reload',
    });
    renderNotes({}); // no initialContent — drawer mounts the section without a prop
    await waitFor(() =>
      expect(screen.getByTestId('notes-body')).toHaveTextContent(
        'Persisted on reload',
      ),
    );
    expect(mockGetProjectNote).toHaveBeenCalledWith({ projectId: 'proj-test' });
  });

  it('1. renders "No notes." placeholder when no content', () => {
    renderNotes({ initialContent: null });
    expect(screen.getByTestId('notes-empty')).toHaveTextContent('No notes.');
  });

  it('2. renders existing notes body', () => {
    renderNotes({ initialContent: 'My telescope setup' });
    expect(screen.getByTestId('notes-body')).toHaveTextContent(
      'My telescope setup',
    );
  });

  it('3. shows Edit button when not read-only', () => {
    renderNotes({ initialContent: 'Some notes' });
    expect(screen.getByRole('button', { name: /edit/i })).toBeInTheDocument();
  });

  it('4. hides Edit button when readOnly=true', () => {
    renderNotes({ initialContent: 'Archived notes', readOnly: true });
    expect(
      screen.queryByRole('button', { name: /edit/i }),
    ).not.toBeInTheDocument();
  });

  it('5. opens textarea on Edit click', async () => {
    renderNotes({ initialContent: 'Some notes' });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /edit/i }));
    });
    expect(screen.getByTestId('notes-textarea')).toBeInTheDocument();
  });

  it('6. Cancel restores original content and hides textarea', async () => {
    renderNotes({ initialContent: 'Original' });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /edit/i }));
    });
    const textarea = screen.getByTestId('notes-textarea');
    fireEvent.change(textarea, { target: { value: 'Modified' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /cancel/i }));
    });
    expect(screen.queryByTestId('notes-textarea')).not.toBeInTheDocument();
    expect(screen.getByTestId('notes-body')).toHaveTextContent('Original');
  });

  it('7. Save button calls saveNote and closes editing', async () => {
    mockSaveNote.mockResolvedValue({ updatedAt: '2026-06-01T12:00:00Z' });
    renderNotes({ initialContent: 'Hello' });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /edit/i }));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /save/i }));
    });
    await waitFor(() => {
      expect(mockSaveNote).toHaveBeenCalledWith('proj-test', 'Hello');
    });
    expect(screen.queryByTestId('notes-textarea')).not.toBeInTheDocument();
  });

  it('8. content too large shows Save button as disabled', async () => {
    renderNotes({ initialContent: '' });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /edit/i }));
    });
    const textarea = screen.getByTestId('notes-textarea');
    // Set content to 1 byte over the limit.
    fireEvent.change(textarea, {
      target: { value: 'x'.repeat(MAX_NOTE_BYTES + 1) },
    });
    // Save button should be disabled (overLimit guard).
    const saveBtn = screen.getByRole('button', { name: /save/i });
    expect(saveBtn).toBeDisabled();
    // Byte counter should be visible and show over-limit count.
    expect(screen.getByTestId('notes-byte-counter')).toBeInTheDocument();
  });

  it('9. byte counter reflects current content size', async () => {
    renderNotes({ initialContent: '' });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /edit/i }));
    });
    const textarea = screen.getByTestId('notes-textarea');
    fireEvent.change(textarea, { target: { value: 'abc' } });
    expect(screen.getByTestId('notes-byte-counter')).toHaveTextContent('3');
  });

  // ── Debounced autosave ──────────────────────────────────────────────────────

  // Uses the REAL use-debounce under fake timers, mirroring
  // SessionNotesSection.test.tsx: a synchronous debounce mock would let a
  // component with no debounce at all pass these tests.
  describe('debounced autosave', () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    async function elapse(ms: number) {
      await act(() => vi.advanceTimersByTimeAsync(ms));
    }

    /** Opens the editor; `initialContent` is passed so no mount fetch runs. */
    async function openEditor(
      props: Partial<React.ComponentProps<typeof ProjectNotesSection>> = {},
    ) {
      renderNotes({ initialContent: '', ...props });
      await act(async () => {
        fireEvent.click(screen.getByRole('button', { name: /edit/i }));
      });
      return screen.getByTestId('notes-textarea');
    }

    it('11. does not save before the debounce interval elapses', async () => {
      mockSaveNote.mockResolvedValue({ updatedAt: '2026-06-01T12:00:00Z' });
      const textarea = await openEditor();
      fireEvent.change(textarea, { target: { value: 'Seeing was poor' } });

      await elapse(NOTE_DEBOUNCE_MS - 1);
      expect(mockSaveNote).not.toHaveBeenCalled();

      await elapse(1);
      expect(mockSaveNote).toHaveBeenCalledExactlyOnceWith(
        'proj-test',
        'Seeing was poor',
      );
      expect(mockAddToast).not.toHaveBeenCalled();

      // The saved signal renders only in the collapsed view, so leaving the
      // editor is what surfaces the autosave's recorded `updatedAt`.
      await act(async () => {
        fireEvent.click(screen.getByRole('button', { name: /cancel/i }));
      });
      expect(screen.getByTestId('notes-saved-indicator')).toBeInTheDocument();
    });

    it('12. coalesces rapid successive edits into a single save', async () => {
      mockSaveNote.mockResolvedValue({ updatedAt: '2026-06-01T12:00:00Z' });
      const textarea = await openEditor();

      for (const value of ['S', 'Se', 'Sei', 'Seei']) {
        fireEvent.change(textarea, { target: { value } });
        await elapse(NOTE_DEBOUNCE_MS - 1);
      }
      expect(mockSaveNote).not.toHaveBeenCalled();

      await elapse(NOTE_DEBOUNCE_MS);
      expect(mockSaveNote).toHaveBeenCalledExactlyOnceWith('proj-test', 'Seei');
    });

    it('13. maps note.content_too_large to the inline field error', async () => {
      mockSaveNote.mockResolvedValue({ error: 'note.content_too_large' });
      const textarea = await openEditor();
      fireEvent.change(textarea, {
        target: { value: 'Within the client cap' },
      });

      await elapse(NOTE_DEBOUNCE_MS);

      expect(screen.getByTestId('notes-field-error')).toHaveTextContent(
        m.projects_notes_byte_limit_exceeded({
          max: MAX_NOTE_BYTES.toLocaleString(),
        }),
      );
      expect(mockAddToast).not.toHaveBeenCalled();
    });

    it('14. maps project.read_only to the archived toast', async () => {
      mockSaveNote.mockResolvedValue({ error: 'project.read_only' });
      const textarea = await openEditor();
      fireEvent.change(textarea, { target: { value: 'Edited after archive' } });

      await elapse(NOTE_DEBOUNCE_MS);

      expect(mockAddToast).toHaveBeenCalledExactlyOnceWith({
        message: m.projects_toast_archived_readonly(),
        variant: 'error',
      });
      expect(screen.queryByTestId('notes-field-error')).not.toBeInTheDocument();
    });

    it('15. maps an unrecognised error to the generic save-failed toast', async () => {
      mockSaveNote.mockResolvedValue({ error: 'db.locked' });
      const textarea = await openEditor();
      fireEvent.change(textarea, { target: { value: 'Anything' } });

      await elapse(NOTE_DEBOUNCE_MS);

      expect(mockAddToast).toHaveBeenCalledExactlyOnceWith({
        message: m.projects_toast_save_notes_failed({ error: 'db.locked' }),
        variant: 'error',
      });
      expect(screen.queryByTestId('notes-field-error')).not.toBeInTheDocument();
    });
  });
});
