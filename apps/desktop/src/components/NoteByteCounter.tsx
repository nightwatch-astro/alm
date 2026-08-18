// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Byte counter for the debounced-autosave note editors (projects — spec 024 —
 * and sessions — #773).
 *
 * `testId` is required rather than defaulted: the two editors ship different
 * ids (`notes-byte-counter`, `session-notes-byte-counter`) and both are asserted
 * on by name, so neither may inherit the other's.
 */

import { MAX_NOTE_BYTES, noteByteStatus } from '@/lib/notes';
import { m } from '@/lib/i18n';

export interface NoteByteCounterProps {
  content: string;
  testId: string;
}

export function NoteByteCounter({ content, testId }: NoteByteCounterProps) {
  const { byteCount, overLimit, nearLimit } = noteByteStatus(content);
  return (
    <span
      data-testid={testId}
      className={
        overLimit
          ? 'pv-project-notes__byte-counter--over'
          : nearLimit
            ? 'pv-project-notes__byte-counter--near'
            : 'pv-project-notes__byte-counter'
      }
    >
      {byteCount.toLocaleString()} / {MAX_NOTE_BYTES.toLocaleString()}{' '}
      {m.projects_notes_bytes_unit()}
    </span>
  );
}
