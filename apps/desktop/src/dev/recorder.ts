// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Developer-mode recording proxy and in-memory call ring buffer (spec 021 US2).
 *
 * When developer mode is on (`devMode = true`), the dispatcher is wrapped so
 * every contract call is captured into a 100-entry FIFO ring buffer.
 *
 * When developer mode is off, the original dispatcher is returned unchanged
 * so there is ZERO overhead in the hot path (FR-008, SC-004).
 *
 * Architecture:
 * - `wrap(dispatch, contracts)` returns a new dispatch function that records calls.
 * - `getCallSnapshot()` returns the current buffer in newest-first order.
 * - `resetRecorder()` clears the buffer (used in tests).
 * - Ring buffer capacity is 100; oldest entries are evicted on overflow.
 * - Payloads larger than 64 KB are truncated (marker set on the record).
 * - Sensitive fields are redacted before storage per ContractMeta.sensitiveFields.
 */

import type { ContractCall, ContractMeta } from '@/bindings/index';
import { errMessage } from '@/lib/errors';
import { safeStringify } from '@/lib/safe-json';

/** Maximum entries retained in the ring buffer. */
export const CALL_BUFFER_SIZE = 100;

/** Maximum payload bytes before truncation (64 KB). */
const MAX_PAYLOAD_BYTES = 64 * 1024;

/** Sensitive field names always redacted (spec 021 A-021-3). */
const ALWAYS_SENSITIVE = new Set(['password', 'token', 'secret', 'api_key']);

// ── Internal state ────────────────────────────────────────────────────────────

interface BufferState {
  /** Entries in insertion order (oldest first, newest last). */
  entries: ContractCall[];
  /** Monotonic counter for id generation. */
  seq: number;
  /** Total entries evicted since session start. */
  dropped: number;
}

const state: BufferState = {
  entries: [],
  seq: 0,
  dropped: 0,
};

type Listener = () => void;
const listeners = new Set<Listener>();

function notify(): void {
  for (const l of listeners) l();
}

// ── Redaction ─────────────────────────────────────────────────────────────────

/**
 * Deep-clone and redact sensitive fields from a payload object.
 *
 * - Fields named in `ALWAYS_SENSITIVE` at any depth → `"<redacted>"`.
 * - String values that look like absolute filesystem paths → `"${PATH}"`.
 *   (Simple heuristic: starts with `/`, `C:\`, `D:\`, etc.)
 */
export function redactPayload(
  payload: unknown,
  sensitiveFields: string[] = [],
): unknown {
  const extraSensitive = new Set(sensitiveFields.map((f) => f.toLowerCase()));
  return redactValue(payload, extraSensitive);
}

function redactValue(value: unknown, extra: Set<string>): unknown {
  if (value === null || value === undefined) return value;
  if (typeof value === 'string') {
    // Redact absolute filesystem paths (Windows/Unix).
    if (/^[A-Za-z]:\\|^\/[^/]/.test(value)) return '${PATH}';
    return value;
  }
  if (Array.isArray(value)) {
    return value.map((item) => redactValue(item, extra));
  }
  if (typeof value === 'object') {
    const result: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      const lower = k.toLowerCase();
      if (ALWAYS_SENSITIVE.has(lower) || extra.has(lower)) {
        result[k] = '<redacted>';
      } else {
        result[k] = redactValue(v, extra);
      }
    }
    return result;
  }
  return value;
}

// ── Payload size check ────────────────────────────────────────────────────────

function serializeAndCheck(value: unknown): {
  value: unknown;
  truncated: boolean;
} {
  const json = safeStringify(value);
  if (json === null) {
    return { value: { error: 'serialize_failed' }, truncated: false };
  }
  if (json.length > MAX_PAYLOAD_BYTES) {
    return {
      value: { _truncated: true, _originalBytes: json.length },
      truncated: true,
    };
  }
  return { value, truncated: false };
}

// ── Ring buffer append ────────────────────────────────────────────────────────

function pushCall(call: ContractCall): void {
  state.entries.push(call);
  if (state.entries.length > CALL_BUFFER_SIZE) {
    state.entries.shift();
    state.dropped += 1;
  }
  notify();
}

// ── Contract name normalization ─────────────────────────────────────────────

/**
 * Explicit aliases for commands whose Tauri fn name has MORE segments than
 * the registry's dotted contract name — the blanket underscore-to-dot
 * fallback below only recovers names where the two shapes are 1:1.
 * `lifecycle.transition` is one logical (replay-unsafe) registry operation
 * split into two backend command variants; both must resolve to the same
 * `ContractMeta`. Add further entries here if another such split appears.
 */
const CONTRACT_NAME_ALIASES: Record<string, string> = {
  lifecycle_transition_apply: 'lifecycle.transition',
  lifecycle_transition_preview: 'lifecycle.transition',
};

/**
 * Normalize a Tauri invoke command name to the dotted contract-registry name.
 *
 * The real dispatcher receives the Rust `#[tauri::command]` fn name
 * (snake_case, e.g. `dev_calls_list`), not the dotted operation name used in
 * `ContractMeta.name` (e.g. `dev.calls.list`) — `tauri-specta` registers
 * commands under the Rust fn name (see `apps/desktop/src-tauri/tests/bindings.rs`
 * "IPC name alignment regression"). Every current registry entry EXCEPT
 * `lifecycle.transition` (see `CONTRACT_NAME_ALIASES`) follows a 1:1
 * `namespace_operation` -> `namespace.operation` shape, so aliases are
 * checked first and a blanket underscore-to-dot replacement is the fallback.
 * Commands outside the dev registry simply fail to match a `ContractMeta`
 * afterward, same as before normalization (spec 021 follow-up #736).
 */
function toContractName(cmd: string): string {
  return CONTRACT_NAME_ALIASES[cmd] ?? cmd.replace(/_/g, '.');
}

// ── Public API ────────────────────────────────────────────────────────────────

/** Subscribe to buffer changes. Returns an unsubscribe function. */
export function subscribeRecorder(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Return all entries in newest-first order. */
export function getCallSnapshot(): ContractCall[] {
  return [...state.entries].reverse();
}

/** Total entries dropped (evicted) since session start. */
export function getDropped(): number {
  return state.dropped;
}

/** Reset buffer state (for tests). */
export function resetRecorder(): void {
  state.entries = [];
  state.seq = 0;
  state.dropped = 0;
  notify();
}

// ── Dispatcher proxy ──────────────────────────────────────────────────────────

/**
 * Tauri dispatch function signature (matches `invoke` from api/commands.ts).
 */
export type DispatchFn = (
  cmd: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;

/**
 * Wrap a Tauri dispatch function so every call is recorded into the ring buffer.
 *
 * When `devMode` is `false`, returns the original dispatcher unchanged
 * (zero-overhead guarantee, FR-008, SC-004).
 *
 * @param dispatch - The original Tauri invoke function.
 * @param devMode  - Whether developer mode is on.
 * @param contracts - Contract registry for sensitive-field lookup.
 */
export function wrap(
  dispatch: DispatchFn,
  devMode: boolean,
  contracts: ContractMeta[] = [],
): DispatchFn {
  if (!devMode) {
    return dispatch;
  }

  // Build a lookup map for sensitive fields by contract name.
  const sensitiveByContract = new Map<string, string[]>();
  for (const c of contracts) {
    sensitiveByContract.set(c.name, c.sensitiveFields ?? []);
  }

  return async function recordingDispatch(
    cmd: string,
    args?: Record<string, unknown>,
  ): Promise<unknown> {
    const startedAt = new Date().toISOString();
    const t0 = performance.now();

    const contract = toContractName(cmd);
    const sensitiveFields = sensitiveByContract.get(contract) ?? [];
    const rawRequest = args ?? {};
    const redactedRequest = redactPayload(rawRequest, sensitiveFields);
    const { value: storedRequest, truncated: reqTruncated } =
      serializeAndCheck(redactedRequest);

    state.seq += 1;
    const id = `call:${state.seq}`;

    let response: unknown;
    let error: ContractCall['error'];
    let resTruncated = false;

    try {
      const raw = await dispatch(cmd, args);
      const redactedRes = redactPayload(raw, sensitiveFields);
      const checked = serializeAndCheck(redactedRes);
      response = checked.value;
      resTruncated = checked.truncated;
    } catch (err: unknown) {
      const e = err as { code?: string; message?: string };
      error = {
        // Diagnostic surface (dev-tools only): record the RAW backend code +
        // message for fidelity. User-facing translation to a catalog message
        // happens once at the display layer via errMessage (spec 046 FR-008),
        // not here. Falls back to errMessage for non-Error throws.
        code: e?.code ?? 'unknown',
        message: e?.message ?? errMessage(err),
      };
    }

    const durationMs = performance.now() - t0;

    const record: ContractCall = {
      id,
      contract,
      contractVersion: '1.0.0',
      request: storedRequest,
      response,
      error,
      startedAt,
      durationMs,
      payloadTruncated: reqTruncated || resTruncated,
    };
    pushCall(record);

    // Best-effort mirror into the backend ring buffer so `dev.export` also
    // observes live calls (spec 021 follow-up #736). Uses the raw dispatcher
    // captured above — NOT `recordingDispatch` itself — so this forwarding
    // call is never recorded (would otherwise recurse). Failures (e.g. the
    // command not yet available, or `devMode` racing off) are swallowed:
    // the JS-side buffer above is the source of truth for the UI.
    if (cmd !== 'dev_calls_push') {
      void dispatch('dev_calls_push', { call: record }).catch(() => {});
    }

    if (error)
      throw Object.assign(new Error(error.message), { code: error.code });
    return response;
  };
}
