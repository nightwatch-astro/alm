// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Modal — shared, parameterised dialog/overlay for the whole app.
 *
 * Wraps `@base-ui-components/react/dialog` (focus-trap + Escape) with the
 * app's standard chrome so feature code never re-implements an overlay:
 *
 *   ┌─────────────────────────────────────────────┐
 *   │ title · subtitle                        ✕    │  ← header (✕ top-right)
 *   ├─────────────────────────────────────────────┤
 *   │ children (scrolls)                          │  ← body
 *   ├─────────────────────────────────────────────┤
 *   │ footer (optional, pinned)                   │  ← footer
 *   └─────────────────────────────────────────────┘
 *
 * Dismissal: clicking the dimmed backdrop (when `closeOnBackdrop`), Escape, or
 * the ✕ closes it. base-ui's automatic `outside-press` is IGNORED — it
 * misclassifies clicks on inner controls that re-render/unmount (checkboxes,
 * foldouts) as outside presses, which would close the modal on every
 * interaction. Genuine outside dismissal is handled by the explicit backdrop
 * click below.
 *
 * Size presets cap the width (`auto` hugs content); the body scrolls within a
 * height cap so tall content never pushes the modal off-screen. This component
 * is the canonical replacement for the ad-hoc per-feature dialog wrappers.
 */

import { useState, type ReactNode, type RefObject } from 'react';
import { Dialog } from '@base-ui-components/react/dialog';
import { m } from '@/lib/i18n';
import {
  backdrop as backdropCls,
  popup as popupCls,
  sizeVariants,
  header as headerCls,
  title as titleCls,
  titleSpacer,
  subtitle as subtitleCls,
  closeBtn,
  body as bodyCls,
  footer as footerCls,
} from '@/ui/modal.css';

export type ModalSize = 'sm' | 'md' | 'lg' | 'xl' | 'auto';

export interface ModalProps {
  /** Controlled open state. */
  open: boolean;
  /** Called when the modal requests close (backdrop / Escape / ✕). */
  onClose: () => void;
  /** Header title. When omitted, the header shows only the ✕. */
  title?: ReactNode;
  /** Secondary line beside the title (e.g. a count summary). */
  subtitle?: ReactNode;
  /**
   * Element to focus when the dialog opens (Base UI `Dialog.Popup`
   * `initialFocus`). DEFAULT (omitted): Base UI's own default — the first
   * tabbable element, which in this chrome is the header ✕ (#841: a bare
   * `autoFocus` on a body field races that default and can lose). Pass a ref
   * to the field that should actually receive focus instead of relying on
   * `autoFocus` inside `children`.
   */
  initialFocus?: RefObject<HTMLElement | null> | boolean;
  /** Modal body — scrolls within the height cap. */
  children: ReactNode;
  /** Optional pinned footer (e.g. action buttons). */
  footer?: ReactNode;
  /** Width preset. DEFAULT `md`. `auto` hugs content. */
  size?: ModalSize;
  /** Accessible label (defaults to the title when it is a string). */
  ariaLabel?: string;
  /** Dismiss on backdrop click. DEFAULT true. */
  closeOnBackdrop?: boolean;
  /** Hide the header ✕ (Escape/backdrop still close). DEFAULT false. */
  hideClose?: boolean;
  /** Extra class on the popup card. */
  className?: string;
  /** Extra class on the scrollable body. */
  bodyClassName?: string;
  /** Test id on the popup card. */
  'data-testid'?: string;
}

export function Modal({
  open,
  onClose,
  title,
  subtitle,
  children,
  footer,
  initialFocus,
  size = 'md',
  ariaLabel,
  closeOnBackdrop = true,
  hideClose = false,
  className,
  bodyClassName,
  'data-testid': testId,
}: ModalProps) {
  const label = ariaLabel ?? (typeof title === 'string' ? title : undefined);

  // Captures whichever element invoked the dialog, so Dialog.Popup's
  // `finalFocus` (base-ui's own return-focus mechanism) has somewhere to
  // send focus back to on close (#844). This component is used without a
  // registered `Dialog.Trigger` (controlled `open`/`onClose`), so base-ui
  // has no trigger to fall back to on its own. Uses React's documented
  // "adjust state during render when a prop changes" pattern (not an
  // effect, and not a ref mutation — react-hooks/refs forbids reading/
  // writing ref.current in the render body): base-ui's own initial-focus
  // effect fires before any effect Modal could register on `open`, so by
  // the time an effect ran here the invoker would already have lost focus
  // to the dialog's first tabbable element. Reading `document.activeElement`
  // synchronously during this render, on the exact render where `open`
  // flips true, still sees the pre-dialog focus.
  const [invoker, setInvoker] = useState<HTMLElement | null>(null);
  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open && typeof document !== 'undefined') {
      setInvoker(document.activeElement as HTMLElement | null);
    }
  }

  const popupClass = [popupCls, sizeVariants[size], className]
    .filter(Boolean)
    .join(' ');
  const bodyClass = [bodyCls, bodyClassName].filter(Boolean).join(' ');

  return (
    <Dialog.Root
      open={open}
      onOpenChange={(isOpen, eventDetails) => {
        if (isOpen) return;
        // Ignore `outside-press` — base-ui treats clicks on re-rendering inner
        // controls as outside presses, which would close the modal mid-edit.
        // Backdrop dismissal is wired explicitly below; Escape ('escape-key')
        // and the ✕ ('close-press') still close here.
        if (eventDetails?.reason === 'outside-press') return;
        onClose();
      }}
    >
      <Dialog.Portal>
        <Dialog.Backdrop
          className={backdropCls}
          onClick={closeOnBackdrop ? onClose : undefined}
        />
        <Dialog.Popup
          className={popupClass}
          aria-label={label}
          data-testid={testId}
          initialFocus={initialFocus}
          finalFocus={() => invoker}
        >
          <div className={headerCls}>
            {title != null ? (
              <Dialog.Title className={titleCls}>{title}</Dialog.Title>
            ) : (
              // Keep the close button right-aligned even without a title.
              <span className={titleSpacer} />
            )}
            {subtitle != null && (
              <span className={subtitleCls}>{subtitle}</span>
            )}
            {!hideClose && (
              <Dialog.Close className={closeBtn} aria-label={m.common_close()}>
                ✕
              </Dialog.Close>
            )}
          </div>

          <div className={bodyClass}>{children}</div>

          {footer != null && <div className={footerCls}>{footer}</div>}
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
