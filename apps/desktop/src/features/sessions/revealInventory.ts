// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Sessions row-Reveal IPC helper (spec 037 caller migration).
 *
 * Moves the inventory-row reveal glue off the hand-written `@/api/commands`
 * wrapper onto the generated `commands.nativeReveal` binding (FR-004: the
 * behaviour is moved, not dropped). The native reveal is tagged with an
 * `inventory_row` audit context so spec-004 / spec-006 FR-007 audit
 * correlation is preserved, and the generated `Result` is unwrapped into the
 * throw-on-error contract the caller toasts on.
 */

import { commands } from '@/bindings/index';
import { unwrap } from '@/api/ipc';
export { resolveRevealPath } from '@/lib/path';

export async function revealInventoryPath(args: {
  path: string;
  sessionId?: string;
}): Promise<void> {
  unwrap(
    await commands.nativeReveal({
      requestId: crypto.randomUUID(),
      path: args.path,
      entityKind: 'inventory_row',
      entityId: args.sessionId ?? null,
    }),
  );
}
