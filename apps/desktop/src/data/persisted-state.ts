// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * persisted-state.ts — shared utility for SQLite-durable UI state (2026-07-24).
 *
 * Architecture decision: ALL persisted frontend state lives in SQLite;
 * localStorage is ONLY a synchronous boot cache. This utility generalises the
 * proven spec-018 theme/fontSize/zoom/locale pattern so every key gets the
 * same treatment for free.
 *
 * ## Write-behind policy
 *
 * `set()` updates in-memory (immediately authoritative), writes localStorage
 * synchronously (if `bootCache` is enabled), and schedules a debounced
 * `settingsUpdate` to SQLite. The SQLite write is fire-and-forget: a failure
 * never reverts the in-memory state, matching the existing
 * `persistThemeToSettings` convention.
 *
 * ## Hydration
 *
 * `hydrateScope(scope)` issues ONE `settingsGet(scope)` call per scope and
 * reconciles in-memory + localStorage from the DB response. `localStorage`
 * stays authoritative until the first successful reconcile (offline-safe).
 * Call it once at boot after Tauri IPC is available — wired in `main.tsx`
 * alongside `hydrateThemeFromSettings`.
 *
 * ## useSyncExternalStore compatibility
 *
 * The `subscribe` and `get` functions returned by `createPersistedState` are
 * directly compatible with `useSyncExternalStore(subscribe, get)`.
 */

// ── Scope registry ────────────────────────────────────────────────────────────
//
// All PersistedState instances register themselves by scope so that
// `hydrateScope` can reconcile all keys for a scope in ONE round-trip.

/** Internal handle stored in the scope registry. */
interface PersistedStateHandle<T> {
  readonly settingsKey: string;
  readonly defaultValue: T;
  setFromDb(value: unknown): void;
}

const scopeRegistry = new Map<string, Set<PersistedStateHandle<unknown>>>();

// All result objects, tracked so __resetScopeRegistryForTest can call
// _resetForTest() on each (clears in-memory value + boot-cache LS key).
const allInstances = new Set<PersistedStateResult<unknown>>();

// Cache of DB values per scope, populated by hydrateScope on a successful
// fetch. Late-registering handles (e.g. lazy picker/grouping instances
// created after hydrateScope returns) consult this cache immediately in
// registerHandle so they reconcile without a second IPC round-trip.
const scopeValueCache = new Map<string, Record<string, unknown>>();

function registerHandle<T>(
  scope: string,
  handle: PersistedStateHandle<T>,
): void {
  let set = scopeRegistry.get(scope);
  if (!set) {
    set = new Set();
    scopeRegistry.set(scope, set);
  }
  (set as Set<PersistedStateHandle<unknown>>).add(
    handle as PersistedStateHandle<unknown>,
  );

  // If this scope was already hydrated, apply the cached value immediately
  // rather than waiting for a second hydrateScope call.
  const cached = scopeValueCache.get(scope);
  if (cached) {
    const dbValue = cached[handle.settingsKey];
    if (dbValue !== undefined && dbValue !== null) {
      handle.setFromDb(dbValue);
    }
  }
}

// ── Tauri IPC helpers ─────────────────────────────────────────────────────────
//
// Memoised dynamic import — same pattern as theme.ts to prevent a Vitest
// dev-mode dynamic-import race when two callers both perform the first
// `import()` of the same mocked specifier concurrently.

let tauriCorePromise: Promise<typeof import('@tauri-apps/api/core')> | null =
  null;
function importTauriCore(): Promise<typeof import('@tauri-apps/api/core')> {
  if (!tauriCorePromise) {
    tauriCorePromise = import('@tauri-apps/api/core');
  }
  return tauriCorePromise;
}

// ── Public API ────────────────────────────────────────────────────────────────

export interface PersistedStateOptions<T> {
  /** In-memory and localStorage default when no stored value exists. */
  default: T;
  /**
   * Write a localStorage boot cache on every `set()` call (default `true`).
   * Set to `false` for state not needed before first paint.
   */
  bootCache?: boolean;
  /**
   * Debounce window for the SQLite write-behind (ms). Default 500.
   * Reducing this makes writes more frequent; increasing it coalesces rapid
   * changes at the cost of a larger window of SQLite staleness.
   */
  debounceMs?: number;
}

export interface PersistedStateResult<T> {
  // Property types (not method declarations) so bare refs like
  // `useSyncExternalStore(state.subscribe, state.get)` don't trip
  // @typescript-eslint/unbound-method — the rule treats property-typed
  // functions as stable values, not unbound methods.

  /** Synchronous read — always returns the current in-memory value. */
  readonly get: () => T;
  /** Update in-memory + localStorage + schedule debounced SQLite write. */
  readonly set: (value: T) => void;
  /**
   * Subscribe to changes. Returns an unsubscribe function.
   * Directly compatible with `useSyncExternalStore`.
   */
  readonly subscribe: (listener: () => void) => () => void;
  /**
   * Reconcile in-memory + localStorage from the DB response object produced by
   * `hydrateScope`. Not called directly by consumers — call `hydrateScope`
   * instead, which issues the batch `settingsGet` and calls this on each key.
   */
  readonly _reconcileFromDbValues: (values: Record<string, unknown>) => void;
  /**
   * Cancel any pending debounced SQLite write. Call from a component's
   * `useEffect` cleanup to avoid timer leaks on unmount, e.g.:
   *
   *   useEffect(() => () => myState.cancelPendingWrite(), []);
   */
  readonly cancelPendingWrite: () => void;
  /**
   * Test-only: reset in-memory value to the constructor default and clear the
   * boot-cache localStorage key. Allows module-level singletons to start fresh
   * between tests without re-importing the module.
   */
  _resetForTest(): void;
}

/**
 * Create a persisted state atom backed by SQLite (durable) and localStorage
 * (synchronous boot cache).
 *
 * Boot-cache localStorage keys use the `pv.ps.` prefix
 * (e.g. `pv.ps.uiState.logPanelExpanded`).
 *
 * @param scope  Settings scope for the SQLite key (e.g. `"ui_state"`).
 * @param key    Stable settings key within the scope (e.g.
 *               `"uiState.logPanelExpanded"`).
 * @param opts   Options including the default value, boot-cache flag, and
 *               debounce window.
 */
export function createPersistedState<T>(
  scope: string,
  key: string,
  opts: PersistedStateOptions<T>,
): PersistedStateResult<T> {
  const bootCache = opts.bootCache !== false;
  const debounceMs = opts.debounceMs ?? 500;
  const lsKey = bootCache ? `pv.ps.${key}` : null;

  // ── In-memory state ────────────────────────────────────────────────────────

  let current: T = readBootCache<T>(lsKey, opts.default);
  const listeners = new Set<() => void>();

  function notify(): void {
    for (const l of listeners) l();
  }

  // ── Debounced SQLite write ─────────────────────────────────────────────────

  // Inline debounce so we can cancel it in cancelPendingWrite / _resetForTest.
  let pendingDebounceTimer: ReturnType<typeof setTimeout> | null = null;

  function persistToDb(value: T): void {
    if (pendingDebounceTimer !== null) {
      clearTimeout(pendingDebounceTimer);
    }
    pendingDebounceTimer = setTimeout(() => {
      pendingDebounceTimer = null;
      void (async () => {
        try {
          const { isTauri } = await importTauriCore();
          if (!isTauri()) return;
          const { updateSettings } = await import('@/data/settingsWrite');
          await updateSettings({ scope, values: { [key]: value } });
        } catch {
          // Best-effort — a DB write failure never blocks or reverts the UI change.
        }
      })();
    }, debounceMs);
  }

  // ── Handle for the scope registry ─────────────────────────────────────────

  const handle: PersistedStateHandle<T> = {
    settingsKey: key,
    defaultValue: opts.default,

    setFromDb(dbValue: unknown): void {
      const parsed = safeParse<T>(dbValue, opts.default);
      if (!shallowEqual(parsed, current)) {
        current = parsed;
        writeBootCache(lsKey, current);
        notify();
      }
    },
  };

  registerHandle(scope, handle);

  // ── Returned API ──────────────────────────────────────────────────────────

  const result: PersistedStateResult<T> = {
    // Arrow function properties (not method shorthands) so that bare refs like
    // `useSyncExternalStore(state.subscribe, state.get)` pass the lint rule
    // `noUnboundMethods` — the functions close over module-scope variables and
    // never use `this`, so the binding is a no-op and arrow syntax is correct.
    get: (): T => current,

    set: (value: T): void => {
      current = value;
      writeBootCache(lsKey, value);
      notify();
      persistToDb(value);
    },

    subscribe: (listener: () => void): (() => void) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },

    _reconcileFromDbValues: (values: Record<string, unknown>): void => {
      handle.setFromDb(values[key]);
    },

    cancelPendingWrite: (): void => {
      if (pendingDebounceTimer !== null) {
        clearTimeout(pendingDebounceTimer);
        pendingDebounceTimer = null;
      }
    },

    _resetForTest(): void {
      // Cancel any pending debounce timer to prevent leaked async operations
      // after test teardown (e.g. the virtualizer-scroll-debounce test).
      if (pendingDebounceTimer !== null) {
        clearTimeout(pendingDebounceTimer);
        pendingDebounceTimer = null;
      }
      if (lsKey) {
        try {
          window.localStorage.removeItem(lsKey);
        } catch {
          /* unavailable — non-fatal */
        }
      }
      current = opts.default;
      notify();
    },
  };

  allInstances.add(result as PersistedStateResult<unknown>);
  return result;
}

// ── Scope hydration ───────────────────────────────────────────────────────────

/**
 * Reconcile all keys registered under `scope` from the SQLite settings DB in
 * ONE `settingsGet` round-trip. No-ops outside Tauri or on IPC failure
 * (localStorage stays authoritative until the next successful call).
 *
 * Fetched values are cached in `scopeValueCache`. Any handle that registers
 * AFTER this call (e.g. a lazily-created picker or grouping state) is
 * reconciled immediately from the cache in `registerHandle`, so registration
 * order is not a correctness requirement.
 *
 * Wired in `main.tsx` alongside `hydrateThemeFromSettings`.
 */
export async function hydrateScope(scope: string): Promise<void> {
  try {
    const { isTauri } = await importTauriCore();
    if (!isTauri()) return;

    const [{ commands }, { unwrap }] = await Promise.all([
      import('@/bindings/index'),
      import('@/api/ipc'),
    ]);

    const data = unwrap(await commands.settingsGet(scope));
    const values = data.values as Record<string, unknown>;

    // Cache for late-registering handles (picker/grouping created on first use).
    scopeValueCache.set(scope, values);

    const handles = scopeRegistry.get(scope);
    if (handles) {
      for (const h of handles) {
        const dbValue = values[h.settingsKey];
        if (dbValue !== undefined && dbValue !== null) {
          h.setFromDb(dbValue);
        }
      }
    }
  } catch {
    // Keep the current localStorage-cached values authoritative.
  }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Read and parse a localStorage boot-cache entry; returns `defaultValue` on miss or error. */
function readBootCache<T>(lsKey: string | null, defaultValue: T): T {
  if (!lsKey) return defaultValue;
  try {
    const raw = window.localStorage.getItem(lsKey);
    if (raw === null) return defaultValue;
    return JSON.parse(raw) as T;
  } catch {
    return defaultValue;
  }
}

/** Write a value to localStorage; silently no-ops on failure. */
function writeBootCache<T>(lsKey: string | null, value: T): void {
  if (!lsKey) return;
  try {
    window.localStorage.setItem(lsKey, JSON.stringify(value));
  } catch {
    // Storage full or unavailable — non-fatal.
  }
}

/**
 * Parse an opaque DB value into `T`; returns `defaultValue` when the DB
 * value is `null`/`undefined`. For JSON-compatible types the DB value is
 * already the parsed form (Tauri deserialized it).
 */
function safeParse<T>(value: unknown, defaultValue: T): T {
  if (value === undefined || value === null) return defaultValue;
  return value as T;
}

/**
 * Shallow equality check to skip notify() when the hydrated DB value matches
 * in-memory, preventing spurious re-renders. Uses JSON for arrays/objects.
 */
function shallowEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== typeof b) return false;
  if (typeof a === 'object') {
    try {
      return JSON.stringify(a) === JSON.stringify(b);
    } catch {
      return false;
    }
  }
  return false;
}

// ── Test helpers ──────────────────────────────────────────────────────────────

/**
 * Reset all persisted state to defaults and clear all scope registrations.
 *
 * Test-only — resets in-memory values + boot-cache localStorage keys for every
 * `createPersistedState` instance created so far, then clears the scope registry.
 * Call in `beforeEach`/`afterEach` to prevent module-level singletons from
 * leaking state between tests.
 */
export function __resetScopeRegistryForTest(): void {
  // Reset in-memory values to defaults (keep instances so future calls reset
  // them again — module singletons are created once per module lifetime).
  for (const instance of allInstances) {
    instance._resetForTest();
  }
  // Clear the scope registry and value cache so hydrateScope re-fetches.
  // Do NOT clear allInstances (they persist across resets).
  scopeRegistry.clear();
  scopeValueCache.clear();
  // Reset the memoised Tauri core promise so mock re-imports work.
  tauriCorePromise = null;
}
