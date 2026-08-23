// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Zod-backed ContractError validation (T118).
 *
 * Kept in a separate module so zod is NOT pulled into the boot chunk:
 * ipc.ts loads this lazily at module-init time and caches the validator.
 * All exports are re-exported from ipc.ts for call-site backwards compat.
 */

import { z } from 'zod';

import { safeStringify } from '@/lib/safe-json';

/** Stand-in for a payload `JSON.stringify` refuses (circular, `BigInt`). */
const UNSERIALIZABLE = '<unserializable>';

/** Zod schema for the mandatory `ContractError` envelope fields (T118). */
export const ContractErrorSchema = z.object({
  code: z.string().min(1),
  message: z.string(),
  severity: z.string(),
  retryable: z.boolean(),
  // `details` is intentionally unknown/unconstrained — it is the free-form
  // blob we are watching for drift; we accept any JSON value here.
  details: z.unknown().optional(),
  fieldErrors: z.array(z.unknown()).optional(),
  recoveryActions: z.array(z.unknown()).optional(),
});

export type ValidatedContractError = z.infer<typeof ContractErrorSchema>;

/**
 * Validate a raw IPC error payload as a `ContractError`.
 * Returns the validated object on success; throws a descriptive
 * `IpcPayloadValidationError` on failure so drift is surfaced immediately.
 */
export function validateContractError(raw: unknown): ValidatedContractError {
  const result = ContractErrorSchema.safeParse(raw);
  if (result.success) {
    return result.data;
  }
  throw new IpcPayloadValidationError(
    'ContractError',
    result.error.flatten().fieldErrors,
    raw,
  );
}

/**
 * Thrown when an IPC payload fails zod validation (T118).
 *
 * The message is built with a guarded stringify: a raw payload that
 * `JSON.stringify` refuses must not make the error reporter throw and lose the
 * original failure.
 */
export class IpcPayloadValidationError extends Error {
  constructor(
    public readonly payloadType: string,
    public readonly fieldErrors: Record<string, string[] | undefined>,
    public readonly raw: unknown,
  ) {
    super(
      `IPC payload validation failed for ${payloadType}: ` +
        (safeStringify(fieldErrors) ?? UNSERIALIZABLE) +
        ` — raw: ${safeStringify(raw) ?? UNSERIALIZABLE}`,
    );
    this.name = 'IpcPayloadValidationError';
  }
}
