// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Spec 037 Phase 1 — proves the generated tauri-specta bindings dispatch through
 * our IPC switcher (api/ipc.ts), not directly through @tauri-apps/api/core, so
 * mock mode and the dev-tools recording proxy keep working (FR-002 / US2).
 */
import { describe, it, expect, afterEach } from 'vitest';
import { commands } from '@/bindings';
import { setInvokeOverride, unwrap } from './ipc';

afterEach(() => setInvokeOverride(null));

describe('generated bindings route through the IPC switcher (FR-002)', () => {
  it('a generated command call is observed by the recording override', async () => {
    const calls: string[] = [];
    setInvokeOverride((cmd) => {
      calls.push(cmd);
      return Promise.resolve([]);
    });
    const res = await commands.sessionsList(null, null);
    expect(calls).toContain('sessions_list');
    expect(res.status).toBe('ok');
  });
});

describe('unwrap (FR-003)', () => {
  it('returns data on ok', () => {
    expect(unwrap({ status: 'ok', data: 42 })).toBe(42);
  });
  it('throws the error on error', () => {
    expect(() => unwrap({ status: 'error', error: new Error('boom') })).toThrow(
      'boom',
    );
  });
});

// ── T118: zod validation at the IPC seam ─────────────────────────────────────
// Validation lives in ipc.validate.ts to keep zod off the boot chunk (T118).
import {
  ContractErrorSchema,
  IpcPayloadValidationError,
  validateContractError,
} from './ipc.validate';

describe('ContractErrorSchema zod validation (T118)', () => {
  const validError = {
    code: 'session.not_found',
    message: 'Session not found',
    severity: 'error',
    retryable: false,
    details: null,
  };

  it('accepts a well-formed ContractError payload', () => {
    const result = ContractErrorSchema.safeParse(validError);
    expect(result.success).toBe(true);
  });

  it('accepts a ContractError with optional details blob', () => {
    const result = ContractErrorSchema.safeParse({
      ...validError,
      details: { sessionId: 'abc', extra: [1, 2] },
      fieldErrors: [{ field: 'id', message: 'required' }],
    });
    expect(result.success).toBe(true);
  });

  it('rejects a payload missing the required code field', () => {
    const bad = { message: 'oops', severity: 'error', retryable: false };
    const result = ContractErrorSchema.safeParse(bad);
    expect(result.success).toBe(false);
  });

  it('rejects a payload with a non-boolean retryable', () => {
    const bad = { ...validError, retryable: 'yes' };
    const result = ContractErrorSchema.safeParse(bad);
    expect(result.success).toBe(false);
  });

  it('validateContractError returns the validated object for a good payload', () => {
    const validated = validateContractError(validError);
    expect(validated.code).toBe('session.not_found');
  });

  it('validateContractError throws IpcPayloadValidationError for a malformed payload', () => {
    expect(() => validateContractError({ notAContractError: true })).toThrow(
      IpcPayloadValidationError,
    );
  });
});

describe('unwrap validates ContractError envelope (T118)', () => {
  const validContractError = {
    code: 'plan.not_found',
    message: 'Plan does not exist',
    severity: 'error',
    retryable: false,
    details: null,
  };

  it('passes through a valid ContractError (throws the original)', () => {
    expect(() =>
      unwrap({ status: 'error', error: validContractError }),
    ).toThrow();
    // The original error object is thrown (not wrapped), so catching it works.
    try {
      unwrap({ status: 'error', error: validContractError });
    } catch (e) {
      expect(e).toBe(validContractError);
    }
  });

  it('throws IpcPayloadValidationError for a malformed object error payload', () => {
    const malformed = { unexpectedKey: true, noCode: 'missing' };
    expect(() => unwrap({ status: 'error', error: malformed })).toThrow(
      IpcPayloadValidationError,
    );
  });

  it('passes through string errors unchanged (non-ContractError path)', () => {
    expect(() =>
      unwrap({ status: 'error', error: 'plain string error' }),
    ).toThrow('plain string error');
  });

  it('passes through Error instances unchanged', () => {
    const err = new Error('native error');
    expect(() => unwrap({ status: 'error', error: err })).toThrow(
      'native error',
    );
  });
});
