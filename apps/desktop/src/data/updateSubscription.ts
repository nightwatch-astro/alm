// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Signed auto-update subscription (spec 051 US10, T058; staged flow #888).
 *
 * The updater is frontend-driven: `checkForUpdate()` calls the
 * `@tauri-apps/plugin-updater` `check()` API directly (the real minisign
 * keypair and signing pipeline are wired — spec 051 SC-009/T059/T060, #762),
 * downloads and signature-verifies any available update immediately, then
 * stops — the install/relaunch step is deferred to an explicit user action
 * (`restartPendingUpdate`), per US10 AS1 and the #888 staged-flow decision.
 * This also resolves #873 (a failed check is a distinct `check-failed` phase,
 * never rendered as "up to date") and #869 (a `relaunch()` failure after a
 * successful `downloadAndInstall()` is a distinct `restart-failed` phase, not
 * "download failed" — the new version is already installed on disk).
 *
 * `startUpdateSubscription()` runs one check at app start; Settings > Advanced
 * subscribes to the resulting state and exposes a "check again" / "restart"
 * affordance depending on phase.
 *
 * In mock/test mode (VITE_USE_MOCKS=true) the Tauri updater/process APIs are
 * unavailable; every exported action is a no-op and the phase stays 'idle'.
 */

const IS_MOCK = import.meta.env.VITE_USE_MOCKS === 'true';

// E2E test-injection seam: a journey spec can set this before navigation to
// seed a specific update phase without a real Tauri updater host. Only
// consulted in `startUpdateSubscription` before the IS_MOCK guard, so it
// has no effect in production or in unit tests that never set it.
declare global {
  interface Window {
    __PV_TEST__?: {
      updateState?: UpdateState;
    };
  }
}

export type UpdatePhase =
  | 'idle'
  | 'checking'
  | 'up-to-date'
  | 'check-failed'
  | 'downloading'
  | 'download-failed'
  | 'ready'
  | 'restart-failed';

export interface UpdateState {
  phase: UpdatePhase;
  /** Update version, once known (from 'checking' onward once found). */
  version?: string;
  body?: string | null;
  /** Present for the '*-failed' phases. */
  error?: string;
}

type Listener = () => void;

const IDLE_STATE: UpdateState = { phase: 'idle' };

let current: UpdateState = IDLE_STATE;
const listeners = new Set<Listener>();

function setState(next: UpdateState): void {
  current = next;
  for (const listener of listeners) listener();
}

/** Extract a display message from a thrown value without pulling in the
 * ContractError catalog (this is a Tauri plugin error, never a backend
 * ContractError). */
function pluginErrMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function getUpdateSnapshot(): UpdateState {
  return current;
}

export function subscribeUpdate(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

let subscribed = false;

/**
 * Check for an update, and if one exists, download + signature-verify it
 * immediately (#888 staged flow). Never installs/relaunches — call
 * `restartPendingUpdate()` for that, on explicit user action.
 *
 * Safe to call again from a 'check-failed' or 'download-failed' state (the
 * #873 retry affordance) or just to re-check while 'up-to-date'.
 */
export async function checkForUpdate(): Promise<void> {
  if (IS_MOCK) return;

  setState({ phase: 'checking' });
  try {
    const { check } = await import('@tauri-apps/plugin-updater');
    const update = await check();
    if (!update) {
      setState({ phase: 'up-to-date' });
      return;
    }

    setState({
      phase: 'downloading',
      version: update.version,
      body: update.body,
    });
    await update.downloadAndInstall();
    setState({ phase: 'ready', version: update.version, body: update.body });
  } catch (err) {
    // Distinguish a failed *check* from a failed *download* so the retry
    // affordance and copy match what actually failed (#873).
    if (current.phase === 'downloading') {
      setState({
        ...current,
        phase: 'download-failed',
        error: pluginErrMessage(err),
      });
    } else {
      setState({ phase: 'check-failed', error: pluginErrMessage(err) });
    }
  }
}

/**
 * Install the staged update by relaunching the app. Only valid from 'ready'
 * or 'restart-failed' (retry) — a no-op otherwise.
 *
 * `downloadAndInstall()` has already succeeded by the time this is callable,
 * so a `relaunch()` failure must never read as "update failed": the new
 * version is already on disk (#869). It surfaces as 'restart-failed' with a
 * manual-restart hint instead.
 */
export async function restartPendingUpdate(): Promise<void> {
  if (IS_MOCK) return;
  if (current.phase !== 'ready' && current.phase !== 'restart-failed') return;

  try {
    const { relaunch } = await import('@tauri-apps/plugin-process');
    await relaunch();
  } catch (err) {
    setState({
      ...current,
      phase: 'restart-failed',
      error: pluginErrMessage(err),
    });
  }
}

/** Start the update-check subscription (idempotent): runs one check now. */
export async function startUpdateSubscription(): Promise<void> {
  if (subscribed) return;
  subscribed = true;
  // E2E override: lets a journey spec seed any update phase before navigation
  // without a real Tauri updater host. Takes effect only when the spec
  // explicitly sets window.__PV_TEST__.updateState via addInitScript.
  if (typeof window !== 'undefined' && window.__PV_TEST__?.updateState) {
    setState(window.__PV_TEST__.updateState);
    return;
  }
  if (IS_MOCK) return;
  await checkForUpdate();
}

/** Reset subscription state so a later `startUpdateSubscription()` re-checks. */
export function stopUpdateSubscription(): void {
  subscribed = false;
  setState(IDLE_STATE);
}

/**
 * Running app semver (#845), e.g. "0.5.0". `null` in mock/browser-only dev —
 * there is no Tauri host to ask.
 */
export async function getRunningVersion(): Promise<string | null> {
  if (IS_MOCK) return null;
  try {
    const { getVersion } = await import('@tauri-apps/api/app');
    return await getVersion();
  } catch (err) {
    console.warn('[updateSubscription] getVersion failed:', err);
    return null;
  }
}
