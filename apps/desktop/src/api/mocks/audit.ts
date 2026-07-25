// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type { AuditEntry } from '@/bindings/types';
import type { AuditFilterDto } from '@/bindings/index';

// --- Inline fixtures for modules not yet created by T015 ---
//
// Each fixture is pinned to its generated binding type (either via a typed
// `const` annotation or a `satisfies` clause).  This makes any drift between a
// mock fixture and the generated `@/bindings` contract a *compile* error rather
// than a silent mock-mode lie (spec 042 US7 T190/T192).

export const mockAuditEntries: AuditEntry[] = [
  {
    id: 'audit-001',
    timestamp: '2026-05-20T22:15:00Z',
    eventType: 'session.confirmed',
    entityType: 'session',
    entityId: 'ses-001',
    fromState: 'needs_review',
    toState: 'confirmed',
    actor: 'user',
    outcome: 'applied',
    detail: 'User confirmed session',
  },
  {
    id: 'audit-002',
    timestamp: '2026-05-20T22:10:00Z',
    eventType: 'plan.approved',
    entityType: 'plan',
    entityId: 'plan-001',
    fromState: 'ready_for_review',
    toState: 'approved',
    actor: 'user',
    outcome: 'applied',
    detail: 'Plan approved',
  },
  {
    id: 'audit-003',
    timestamp: '2026-05-20T21:45:00Z',
    eventType: 'plan.applied',
    entityType: 'plan',
    entityId: 'plan-001',
    fromState: 'approved',
    toState: 'applied',
    actor: 'system',
    outcome: 'applied',
    detail: 'All 12 items applied',
  },
  {
    id: 'audit-004',
    timestamp: '2026-05-19T23:30:00Z',
    eventType: 'scan.completed',
    entityType: 'root',
    entityId: 'root-001',
    actor: 'system',
    outcome: 'ok',
    detail: 'Discovered 1,247 files in 4.2s',
  },
  {
    id: 'audit-005',
    timestamp: '2026-05-19T23:25:00Z',
    eventType: 'scan.started',
    entityType: 'root',
    entityId: 'root-001',
    actor: 'user',
    outcome: 'ok',
    detail: 'Manual scan triggered',
  },
];

/**
 * Mirrors the real `audit_list`/`audit_export` filter semantics
 * (`apps/desktop/src-tauri/src/commands/audit.rs`) over the mock fixture, so
 * mock mode exercises the same search/entity/outcome/date-range filtering the
 * real `audit_log_entry` query applies. `severity` has no equivalent on the
 * `AuditEntry` fixture (the real DTO doesn't carry it either — only the
 * filter does) and is ignored here, same as it plays no role in what the UI
 * renders.
 */
export function filterMockAuditEntries(
  filters: AuditFilterDto | null | undefined,
): AuditEntry[] {
  let result = mockAuditEntries;
  if (filters?.entityType) {
    result = result.filter((e) => e.entityType === filters.entityType);
  }
  if (filters?.entityId) {
    result = result.filter((e) => e.entityId === filters.entityId);
  }
  if (filters?.outcome) {
    result = result.filter((e) => e.outcome === filters.outcome);
  }
  if (filters?.search) {
    const q = filters.search.toLowerCase();
    result = result.filter(
      (e) =>
        e.eventType.toLowerCase().includes(q) ||
        e.entityType.toLowerCase().includes(q) ||
        e.entityId.toLowerCase().includes(q) ||
        e.actor.toLowerCase().includes(q),
    );
  }
  if (filters?.from) {
    const from = new Date(filters.from).getTime();
    result = result.filter((e) => new Date(e.timestamp).getTime() >= from);
  }
  if (filters?.to) {
    const to = new Date(filters.to).getTime();
    result = result.filter((e) => new Date(e.timestamp).getTime() < to);
  }
  return [...result].sort(
    (a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime(),
  );
}
