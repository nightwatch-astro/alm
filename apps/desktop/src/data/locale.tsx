// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

// Locale runtime — persisted application-language preference (spec 061).
//
// Paraglide resolves the active locale through the strategy chain configured
// in vite.config.ts: `["custom-almSettings", "preferredLanguage",
// "baseLocale"]` (research D1). This module implements the
// `"custom-almSettings"` link via `defineCustomClientStrategy` and everything
// that sits on top of it:
//
//   - `getLocale()`  → localStorage mirror (`pv.locale`), SYNCHRONOUS — the
//     Paraglide runtime requires a synchronous `getLocale` from client
//     strategies (research D3).
//   - `setLocale()`  → settings DB (`general` scope, `locale` key, spec 018)
//     AND the mirror. The DB write is fire-and-forget, mirroring
//     `apps/desktop/src/data/theme.ts`'s `persistThemeToSettings` — it never
//     blocks or reverts the UI on failure.
//   - `hydrateLocaleFromSettings()` → reconciles the mirror against the DB
//     once IPC is available; the DB wins on disagreement (research D3).
//
// `registerLocaleStrategy()` MUST run before Paraglide resolves a locale for
// the first time — mirrors `theme.ts`'s `initAppearance()` (call once at
// boot, before first render). Until it is wired into the app's boot sequence,
// the chain still degrades safely to `preferredLanguage` / `baseLocale`
// (`customClientStrategies.has()` guards the missing-handler case in the
// generated runtime), it just never sees a saved user choice.
//
// `LocaleProvider`/`useLocale` hold the active locale in React state rather
// than the `useSyncExternalStore` singleton theme.ts uses: Paraglide messages
// compile to plain function calls that re-evaluate `getLocale()` on every
// invocation, so re-rendering the subtree (D2) is both necessary and
// sufficient — no external store needed. `changeLocale()` passes
// `{ reload: false }` to `setLocale` — mandatory (research D2): the default
// reload would drop scroll position, open panels, and unsaved edits on a
// Settings-initiated language change.

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import {
  baseLocale as BASE_LOCALE,
  defineCustomClientStrategy,
  getLocale as paraglideGetLocale,
  isLocale,
  locales as SHIPPED_LOCALES,
  setLocale as paraglideSetLocale,
  type Locale,
} from '@/paraglide/runtime';

export type { Locale };
export { BASE_LOCALE, SHIPPED_LOCALES };

const LOCALE_KEY = 'pv.locale';
const SETTINGS_SCOPE = 'general';
const SETTINGS_KEY = 'locale';
const STRATEGY_NAME = 'custom-almSettings';

/**
 * Memoized dynamic imports of the Tauri core API and the IPC boundary —
 * same shape (and same dev-mode dynamic-import race rationale) as `theme.ts`'s
 * `importTauriCore`. Duplicated rather than shared: `theme.ts` is outside
 * this module's scope (spec 061 p1 owns `src/data/locale*` only).
 *
 * Memoizing `@/bindings/index`/`@/api/ipc` too (which `theme.ts` does not)
 * is load-bearing here, not just tidiness: Paraglide's `getLocale()` runtime
 * self-persists whatever it resolves on its own first-ever call
 * (opral/inlang-paraglide-js#455), firing `persistLocaleToSettings` in the
 * background. That can race a concurrent `hydrateLocaleFromSettings()` call
 * (e.g. `LocaleProvider`'s mount effect) — two independent first-time
 * `import()`s of the same specifier, which vitest's mocked module runner can
 * resolve inconsistently under concurrency. A shared in-flight promise
 * removes the second import entirely instead of racing it.
 */
let tauriCorePromise: Promise<typeof import('@tauri-apps/api/core')> | null =
  null;
function importTauriCore(): Promise<typeof import('@tauri-apps/api/core')> {
  if (!tauriCorePromise) {
    tauriCorePromise = import('@tauri-apps/api/core');
  }
  return tauriCorePromise;
}

let bindingsPromise: Promise<typeof import('@/bindings/index')> | null = null;
function importBindings(): Promise<typeof import('@/bindings/index')> {
  if (!bindingsPromise) {
    bindingsPromise = import('@/bindings/index');
  }
  return bindingsPromise;
}

let ipcPromise: Promise<typeof import('@/api/ipc')> | null = null;
function importIpc(): Promise<typeof import('@/api/ipc')> {
  if (!ipcPromise) {
    ipcPromise = import('@/api/ipc');
  }
  return ipcPromise;
}

/**
 * In-memory fallback for the locale mirror.
 *
 * When localStorage is unavailable (SecurityError in a sandboxed iframe,
 * storage blocked by the browser, or a throwing stub in tests), the
 * custom-almSettings strategy would return undefined and Paraglide would
 * fall through to preferredLanguage/baseLocale — diverging from the React
 * state and document.lang already set by changeLocale. This variable is
 * set synchronously by setLocaleMirror before the localStorage write is
 * attempted, so getLocaleMirror always returns the last user-selected locale
 * even when storage is entirely unavailable. The selection survives the
 * session but not a restart (no durable write occurred).
 */
let inMemoryLocale: string | undefined;

/**
 * True when the most recent localStorage.setItem(LOCALE_KEY, …) threw. Gates
 * the readable-but-empty fallback path in getLocaleMirror: only a broken
 * WRITE justifies shadowing an empty (authoritative) storage state.
 */
let localeWriteFailed = false;

/** Synchronous mirror read, validated against the shipped locale set. */
function getLocaleMirror(): Locale | undefined {
  try {
    const v = localStorage.getItem(LOCALE_KEY);
    if (v && isLocale(v)) {
      // Durable storage is authoritative; drop the in-memory shadow so it
      // can never mask a later authoritative state.
      inMemoryLocale = undefined;
      return v;
    }
    // localStorage is READABLE and empty. Two cases:
    // - the last setItem FAILED (write-broken storage): the in-memory value
    //   is the user's live selection — serve it so Paraglide stays coherent
    //   with the React state (the original tlw.14 bug).
    // - the last setItem SUCCEEDED (or never ran): empty storage is
    //   authoritative — a cleared storage (user reset, test isolation via
    //   localStorage.clear()) must win over a stale in-memory value.
    if (localeWriteFailed && inMemoryLocale && isLocale(inMemoryLocale)) {
      return inMemoryLocale;
    }
    inMemoryLocale = undefined;
    return undefined;
  } catch {
    // localStorage entirely unavailable — serve the in-memory value so the
    // Paraglide strategy and React state stay coherent for the session.
    return inMemoryLocale && isLocale(inMemoryLocale)
      ? inMemoryLocale
      : undefined;
  }
}

/**
 * Best-effort write-through to the settings DB (`general` scope, `locale`
 * key) — mirrors `theme.ts`'s `persistThemeToSettings`. Outside Tauri (dev
 * server, vitest) or on any IPC failure this silently no-ops; the
 * localStorage mirror is written synchronously by the caller regardless.
 */
function persistLocaleToSettings(locale: string): void {
  void (async () => {
    try {
      const { isTauri } = await importTauriCore();
      if (!isTauri()) return;

      const { updateSettings } = await import('@/data/settingsWrite');
      await updateSettings({
        scope: SETTINGS_SCOPE,
        values: { [SETTINGS_KEY]: locale },
      });
    } catch {
      // Best-effort — a DB write failure never blocks or reverts the UI change.
    }
  })();
}

/**
 * The handler registered against Paraglide's `"custom-almSettings"` client
 * strategy. `getLocale` MUST stay synchronous (research D3) — Paraglide's
 * resolver skips a strategy whose handler returns a Promise. `setLocale`
 * writes the mirror synchronously before returning, so any `getLocale()`
 * call made immediately after resolves to the new value even though the DB
 * write is still in flight.
 */
function setLocaleMirror(newLocale: string): void {
  // Always update in-memory first so getLocaleMirror() returns the new value
  // immediately, even if the localStorage write below throws.
  inMemoryLocale = newLocale;
  try {
    localStorage.setItem(LOCALE_KEY, newLocale);
    localeWriteFailed = false;
  } catch {
    /* localStorage may be unavailable — in-memory fallback is sufficient for
       the session; the selection will not survive a restart */
    localeWriteFailed = true;
  }
  persistLocaleToSettings(newLocale);
}

let strategyRegistered = false;

/**
 * Registers the `"custom-almSettings"` client strategy. Idempotent — safe to
 * call more than once (module re-imports under HMR, multiple test files in
 * the same vitest worker). Call once at boot, before the first render
 * (mirrors `theme.ts`'s `initAppearance()`).
 */
export function registerLocaleStrategy(): void {
  if (strategyRegistered) return;
  strategyRegistered = true;
  defineCustomClientStrategy(STRATEGY_NAME, {
    getLocale: getLocaleMirror,
    setLocale: setLocaleMirror,
  });
}

/** The currently resolved locale, per the full strategy chain. */
export function getCurrentLocale(): Locale {
  return canonicalLocale(paraglideGetLocale());
}

/** Keep browser language metadata constrained to the shipped locale set. */
function canonicalLocale(candidate: unknown): Locale {
  return typeof candidate === 'string' && isLocale(candidate)
    ? candidate
    : BASE_LOCALE;
}

function syncDocumentLanguage(candidate: unknown): Locale {
  const locale = canonicalLocale(candidate);
  if (typeof document !== 'undefined') {
    document.documentElement.lang = locale;
  }
  return locale;
}

/**
 * Reconciles the localStorage mirror against the settings DB (spec 018) —
 * the DB is the durable source of truth (research D3); the mirror only
 * exists so `getLocale()` can answer synchronously before this resolves.
 * Returns the corrected locale if the DB disagreed with the mirror, or
 * `undefined` if they already agreed (or the check could not be completed).
 * No-ops outside Tauri or on any IPC failure — the mirror stays authoritative
 * until the next successful reconcile.
 */
export async function hydrateLocaleFromSettings(): Promise<Locale | undefined> {
  try {
    const { isTauri } = await importTauriCore();
    if (!isTauri()) return undefined;

    const [{ commands }, { unwrap }] = await Promise.all([
      importBindings(),
      importIpc(),
    ]);
    const data = unwrap(await commands.settingsGet(SETTINGS_SCOPE));
    const values = data.values as Record<string, unknown>;

    const stored = values[SETTINGS_KEY];
    if (
      typeof stored === 'string' &&
      isLocale(stored) &&
      stored !== getLocaleMirror()
    ) {
      // Reuse the public setLocale path so the mirror, the runtime's own
      // locale resolution, and this hydration step never diverge in how a
      // locale gets applied.
      await paraglideSetLocale(stored, { reload: false });
      syncDocumentLanguage(stored);
      return stored;
    }
    return undefined;
  } catch {
    // Best-effort — keep the current mirror-cached locale.
    return undefined;
  }
}

interface LocaleContextValue {
  locale: Locale;
  /** Applies `next` without a reload (research D2) and re-renders the subtree. */
  changeLocale: (next: Locale) => void;
}

const LocaleContext = createContext<LocaleContextValue | undefined>(undefined);

/**
 * Holds the active locale in React state and re-renders its subtree on
 * change (research D2 — Paraglide messages are plain function calls, so
 * nothing updates without a re-render). Hydrates from the settings DB once
 * on mount; the effect is safe to run more than once (idempotent DB read)
 * and cancels its own state update if unmounted first.
 */
export function LocaleProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(() =>
    syncDocumentLanguage(getCurrentLocale()),
  );

  useEffect(() => {
    let cancelled = false;
    void hydrateLocaleFromSettings().then((corrected) => {
      if (!cancelled && corrected) setLocaleState(corrected);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const changeLocale = useCallback((next: Locale) => {
    const canonical = canonicalLocale(next);
    void paraglideSetLocale(canonical, { reload: false });
    syncDocumentLanguage(canonical);
    setLocaleState(canonical);
  }, []);

  const value = useMemo(
    () => ({ locale, changeLocale }),
    [locale, changeLocale],
  );

  return (
    <LocaleContext.Provider value={value}>{children}</LocaleContext.Provider>
  );
}

/** `{ locale, changeLocale }` — must be called within a `LocaleProvider`. */
export function useLocale(): LocaleContextValue {
  const ctx = useContext(LocaleContext);
  if (!ctx) {
    throw new Error('useLocale must be used within a LocaleProvider');
  }
  return ctx;
}
