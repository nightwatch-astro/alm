// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * `useTargetDetailMutations` — the alias add/remove, display-alias set/clear,
 * and observing-notes (spec 023 US4) edit state + mutation handlers behind
 * `TargetDetail`. Split out of the component (refactor sweep #982) so the
 * render stays render-only; this hook owns every local draft/editing/error
 * field these three edit surfaces need.
 */

import { useCallback, useEffect, useState } from 'react';
import type { ContractError } from '@/lib/errors';
import {
  useAddTargetAlias,
  useRemoveTargetAlias,
  useSetTargetDisplayAlias,
  useClearTargetDisplayAlias,
  useUpdateTargetNotes,
} from './store';
import { m } from '@/lib/i18n';
import { errorMessage } from './target-error-message';

export function useTargetDetailMutations({
  targetId,
  serverDisplayAlias,
  notes,
  onMutated,
}: {
  targetId: string;
  /** The server's current `displayAlias`, once the detail has loaded. */
  serverDisplayAlias: string | null | undefined;
  /** The server's current observing notes value. */
  notes: string | null;
  /**
   * #658: called after an alias add/remove or display-alias set/clear
   * mutation succeeds, so the caller can refetch the list payload the
   * Targets page filters/renders from — otherwise a fresh user alias stays
   * unsearchable and a new display label never propagates to the list row
   * until an unrelated remount refetches it.
   */
  onMutated?: () => void;
}) {
  const [aliasInput, setAliasInput] = useState('');
  const [aliasError, setAliasError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [displayAliasInput, setDisplayAliasInput] = useState('');
  const [displayAliasEditing, setDisplayAliasEditing] = useState(false);

  // US4: observing notes (draft/editing/save-status stay local; the persisted
  // value is TanStack-Query-backed via useTargetNotes/useUpdateTargetNotes).
  const [notesEditing, setNotesEditing] = useState(false);
  const [notesDraft, setNotesDraft] = useState('');
  const [notesSaving, setNotesSaving] = useState(false);
  const [notesSaved, setNotesSaved] = useState(false);
  const [notesError, setNotesError] = useState<string | null>(null);

  // Sync the display-alias draft from the server value whenever a fresh
  // detail lands (mount, post-mutation refetch, or a display-alias mutation's
  // own cache write) — guarded on `!displayAliasEditing` so an in-progress
  // edit is never clobbered by a background refetch (mirrors
  // ProjectNotesSection's initialContent-sync convention).
  useEffect(() => {
    if (serverDisplayAlias !== undefined && !displayAliasEditing) {
      setDisplayAliasInput(serverDisplayAlias ?? '');
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serverDisplayAlias]);

  // Sync the notes draft from the server value whenever it changes, and reset
  // editing/saved/error state when the target itself changes.
  useEffect(() => {
    if (!notesEditing) setNotesDraft(notes ?? '');
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [notes]);

  useEffect(() => {
    setNotesEditing(false);
    setNotesSaved(false);
    setNotesError(null);
  }, [targetId]);

  const addAliasMutation = useAddTargetAlias();
  const removeAliasMutation = useRemoveTargetAlias();
  const setDisplayAliasMutation = useSetTargetDisplayAlias();
  const clearDisplayAliasMutation = useClearTargetDisplayAlias();
  const updateNotesMutation = useUpdateTargetNotes();

  // US4: save notes handler.
  const handleNotesSave = useCallback(async () => {
    setNotesSaving(true);
    setNotesError(null);
    try {
      const { notes: saved } = await updateNotesMutation.mutateAsync({
        targetId,
        notes: notesDraft,
      });
      setNotesDraft(saved ?? '');
      setNotesEditing(false);
      setNotesSaved(true);
    } catch {
      setNotesError(m.sessions_notes_save_failed());
    } finally {
      setNotesSaving(false);
    }
  }, [targetId, notesDraft, updateNotesMutation]);

  // Add user alias.
  const handleAliasAdd = useCallback(async () => {
    const alias = aliasInput.trim();
    if (!alias) {
      setAliasError(m.targets_detail_alias_blank());
      return;
    }
    setAliasError(null);
    try {
      await addAliasMutation.mutateAsync({ targetId, alias });
      setAliasInput('');
      onMutated?.();
    } catch (err) {
      const e = err as ContractError;
      setAliasError(errorMessage(e, m.targets_detail_add_alias_failed()));
    }
  }, [targetId, aliasInput, addAliasMutation, onMutated]);

  // Remove user alias by id.
  const handleAliasRemove = useCallback(
    async (aliasId: string) => {
      setActionError(null);
      try {
        await removeAliasMutation.mutateAsync({ targetId, aliasId });
        onMutated?.();
      } catch (err) {
        const e = err as ContractError;
        setActionError(errorMessage(e, m.targets_detail_remove_alias_failed()));
      }
    },
    [targetId, removeAliasMutation, onMutated],
  );

  // Set display alias.
  const handleDisplayAliasSet = useCallback(async () => {
    setActionError(null);
    try {
      const data = await setDisplayAliasMutation.mutateAsync({
        targetId,
        displayAlias: displayAliasInput.trim(),
      });
      setDisplayAliasInput(data.displayAlias ?? '');
      setDisplayAliasEditing(false);
      onMutated?.();
    } catch (err) {
      const e = err as ContractError;
      setActionError(
        errorMessage(e, m.targets_detail_set_display_alias_failed()),
      );
    }
  }, [targetId, displayAliasInput, setDisplayAliasMutation, onMutated]);

  // Clear display alias.
  const handleDisplayAliasClear = useCallback(async () => {
    setActionError(null);
    try {
      await clearDisplayAliasMutation.mutateAsync({ targetId });
      setDisplayAliasInput('');
      setDisplayAliasEditing(false);
      onMutated?.();
    } catch (err) {
      const e = err as ContractError;
      setActionError(
        errorMessage(e, m.targets_detail_clear_display_alias_failed()),
      );
    }
  }, [targetId, clearDisplayAliasMutation, onMutated]);

  return {
    aliasInput,
    setAliasInput,
    aliasError,
    actionError,
    displayAliasInput,
    setDisplayAliasInput,
    displayAliasEditing,
    setDisplayAliasEditing,
    notesEditing,
    setNotesEditing,
    notesDraft,
    setNotesDraft,
    notesSaving,
    notesSaved,
    setNotesSaved,
    notesError,
    setNotesError,
    handleNotesSave,
    handleAliasAdd,
    handleAliasRemove,
    handleDisplayAliasSet,
    handleDisplayAliasClear,
  };
}
