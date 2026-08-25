// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Single IPC dispatch point (spec 037).
 *
 * The generated tauri-specta bindings (`bindings/index.ts`) are post-processed
 * to import their `invoke` from THIS module instead of `@tauri-apps/api/core`
 * (see `apps/desktop/src-tauri/tests/bindings.rs`). That makes every generated
 * `commands.*` call inherit:
 *   - mock mode (`VITE_USE_MOCKS`) → `mocks.ts` `mockInvoke`, and
 *   - the spec-021 dev-tools recording proxy via `setInvokeOverride`.
 *
 * Keep this module dependency-light: it must not import `commands.ts` (which
 * imports from here), and it loads mocks / the real tauri core lazily.
 */

// ── T118: Zod validation for dynamic/drift-prone IPC payloads ──────────────
//
// Validation types live in `ipc.validate.ts` so zod is NOT bundled in the
// boot chunk (zod enters through the error path only, never at startup).
// `ipc.ts` pre-warms the validate module with a fire-and-forget dynamic
// import and caches the synchronous validator function for use in `unwrap`.
// On a cache miss (error arrives before the tiny validate chunk has loaded —
// practically impossible post-boot) validation is skipped and the raw error
// is rethrown unchanged, which is still better than a silent undefined.

// Cached synchronous validator populated once the validate chunk loads.
let _validate: ((raw: unknown) => void) | null = null;

// Pre-warm the validate chunk at module-init time (lazy, non-blocking).
// This runs before any IPC command could possibly return an error response,
// so the validator is cached long before `unwrap` ever needs it.
void import('./ipc.validate').then((m) => {
  _validate = (raw) => m.validateContractError(raw);
});

const useMocks = import.meta.env.VITE_USE_MOCKS === 'true';

/**
 * Optional recording-proxy override installed by the dev-tools bootRecorder
 * (spec 021, T075 / SC-002). Null in release builds (zero overhead).
 */
let _invokeOverride:
  | ((cmd: string, args?: Record<string, unknown>) => Promise<unknown>)
  | null = null;

/** Install a recording proxy over the IPC dispatcher (dev-tools builds only). */
export function setInvokeOverride(
  fn:
    | ((cmd: string, args?: Record<string, unknown>) => Promise<unknown>)
    | null,
): void {
  _invokeOverride = fn;
}

/**
 * Dispatch an IPC command. Signature-compatible with `@tauri-apps/api/core`'s
 * `invoke` so the generated bindings can call it unchanged.
 */
export async function invoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (_invokeOverride) {
    return _invokeOverride(cmd, args) as Promise<T>;
  }
  if (useMocks) {
    const { mockInvoke } = await import('./mocks');
    // `mockInvoke` returns `Promise<unknown>`; the caller picks the concrete `T`
    // from the generated bindings, so narrowing happens here at the boundary.
    return mockInvoke(cmd, args) as Promise<T>;
  }
  const { invoke: tauriInvoke } = await import('@tauri-apps/api/core');
  return tauriInvoke<T>(cmd, args);
}

/** A generated tauri-specta `Result` value. */
export type IpcResult<T, E = unknown> =
  | { status: 'ok'; data: T }
  | { status: 'error'; error: E };

/** The only keys a contract domain error payload may carry. */
const DOMAIN_ERROR_KEYS = new Set(['code', 'message', 'details']);

/**
 * True when the payload is a contract *domain* error rather than a
 * `ContractError`.
 *
 * `ContractError` always carries `severity` and `retryable`; domain error types
 * such as `ManifestOpError` carry only `code`, `message`, and optional
 * `details`. Running the ContractError schema over one of those replaces the
 * real backend failure with an `IpcPayloadValidationError`, so they are thrown
 * unvalidated. A payload with a foreign key set is still validated, which keeps
 * drift detection intact.
 */
function isDomainErrorPayload(err: object): boolean {
  const record = err as Record<string, unknown>;
  return (
    typeof record.code === 'string' &&
    typeof record.message === 'string' &&
    Object.keys(record).every((key) => DOMAIN_ERROR_KEYS.has(key))
  );
}

/**
 * Translate a generated `Result` into the throw-on-error contract the app's
 * call sites expect: returns `data` on success, throws `error` otherwise.
 *
 * T118: When the error branch carries a `ContractError`-shaped payload, the
 * mandatory envelope fields are validated via the cached validator from
 * `ipc.validate.ts` before re-throwing. This surfaces backend drift (e.g. a
 * renamed `code` key, a missing `message`) as a clear
 * `IpcPayloadValidationError` rather than a silent `undefined` downstream.
 *
 * Plain string errors (older commands), `Error` instances, and contract domain
 * errors are passed through unchanged so no existing behaviour is broken.
 */
export function unwrap<T, E = unknown>(result: IpcResult<T, E>): T {
  if (result.status === 'ok') {
    return result.data;
  }
  const err = result.error;
  // Validate object-shaped errors as ContractError envelopes (T118).
  if (
    err !== null &&
    typeof err === 'object' &&
    !Array.isArray(err) &&
    !(err instanceof Error) &&
    !isDomainErrorPayload(err)
  ) {
    // Use the cached validator if loaded; skip validation on a cache miss
    // (validate chunk not yet loaded — safe to skip, still rethrows the error).
    _validate?.(err);
  }
  throw err;
}
