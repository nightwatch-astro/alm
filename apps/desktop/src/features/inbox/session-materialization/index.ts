// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Inbox session materialization feature barrel (spec 062, US1).
 *
 * Public surface: the panel component and flow hook. Types and IPC seam
 * are internal to this feature directory.
 */

export { SessionMaterializationPanel } from './SessionMaterializationPanel';
export type { SessionMaterializationPanelProps } from './SessionMaterializationPanel';
export { useSessionMaterializationFlow } from './useSessionMaterializationFlow';
export type {
  UseSessionMaterializationFlowResult,
  FlowPhase,
  SessionMaterializationFlowState,
} from './useSessionMaterializationFlow';
