// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Interaction testing convention:
 *
 *   - Use `setupUser()` (and `await user.*`) for anything user-driven:
 *     clicks, keyboard input, typing into fields, keyboard navigation.
 *   - Use `fireEvent` only for synthetic/programmatic events that bypass the
 *     pointer/keyboard stack — e.g. custom DOM events, simulating browser
 *     internals, or components that cannot receive focus (mocks, portals).
 *
 * Why: userEvent simulates the full browser event sequence (pointerdown →
 * mousedown → focus → click …), which catches focus-order bugs, keybinding
 * regressions, and a11y issues that fireEvent's single-event shortcut misses.
 */

import userEvent from '@testing-library/user-event';

/**
 * Returns a pre-configured userEvent instance. Call once per test, then
 * `await user.click(el)` / `await user.keyboard('{ArrowDown}')` etc.
 *
 * Options override the defaults for the rare test that needs them (e.g.
 * `{ delay: null }` to skip artificial typing delays in CI).
 */
export function setupUser(options?: Parameters<typeof userEvent.setup>[0]) {
  return userEvent.setup(options);
}
