// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * ConfirmModal — shared confirmation dialog for destructive/reversible actions.
 *
 * Wraps `Modal` with the standard confirm-dialog chrome: a message body, an
 * optional inline error, optional extra content (for disambiguation UI like a
 * fallback-site selector), and a footer with Cancel + action button.
 *
 * Extracted from six duplicated confirm dialogs across Equipment, DataSources,
 * ObservingSites, MasterArchiveFlow, and SessionDetail (C-13). All shared the
 * same skeleton: size="sm", hideClose, footer with ghost Cancel + typed action
 * button, optional inline field-error.
 */

import type { ReactNode } from 'react';
import { Modal } from './Modal';
import { Btn } from '@/ui';
import { m } from '@/lib/i18n';

export type ConfirmModalActionVariant = 'destructive' | 'danger';

export interface ConfirmModalProps {
  /** Controlled open state. */
  open: boolean;
  /** Called when the modal requests close (Cancel / Escape / backdrop). */
  onClose: () => void;
  /** Dialog title. */
  title: ReactNode;
  /**
   * Body message shown below the title. Rendered inside
   * `<p className="pv-modal__message">`.
   */
  message: ReactNode;
  /** Label for the action (confirm) button. */
  actionLabel: ReactNode;
  /**
   * Visual variant for the action button. Use `destructive` for irreversible
   * actions (delete, archive) and `danger` for reversible ones (disable).
   * DEFAULT `destructive`.
   */
  actionVariant?: ConfirmModalActionVariant;
  /** Whether the action is in progress (disables both buttons). */
  busy?: boolean;
  /** Called when the user confirms the action. */
  onConfirm: () => void;
  /**
   * Inline error text displayed below the message. Typically the error from
   * a failed action attempt (e.g. `root.has_dependents`).
   */
  error?: string | null;
  /**
   * Optional extra content rendered between the message and the footer (e.g.
   * a fallback destination picker when a delete requires choosing a replacement).
   */
  children?: ReactNode;
  /** data-testid for the modal popup. */
  'data-testid'?: string;
}

/**
 * Standard two-button confirmation dialog. Intended for one-click destructive
 * or reversible actions that need a confirmation step — not for multi-step
 * flows or typed-confirmation patterns (use a bespoke Modal for those).
 */
export function ConfirmModal({
  open,
  onClose,
  title,
  message,
  actionLabel,
  actionVariant = 'destructive',
  busy = false,
  onConfirm,
  error,
  children,
  'data-testid': testId,
}: ConfirmModalProps) {
  return (
    <Modal
      open={open}
      onClose={onClose}
      title={title}
      size="sm"
      hideClose
      data-testid={testId}
      footer={
        <>
          <Btn variant="ghost" onClick={onClose} disabled={busy}>
            {m.common_cancel()}
          </Btn>
          <Btn
            variant={actionVariant}
            onClick={onConfirm}
            disabled={busy}
            data-testid={testId ? `${testId}-confirm-btn` : undefined}
          >
            {actionLabel}
          </Btn>
        </>
      }
    >
      <p className="pv-modal__message">{message}</p>
      {children}
      {error && <span className="pv-field-error">{error}</span>}
    </Modal>
  );
}
