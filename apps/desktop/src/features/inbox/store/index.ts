// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Inbox store barrel — re-exports all sub-modules so existing `./store`
 * imports resolve without changes.
 */

export {
  inboxClassifyQueryKey,
  useInboxClassification,
  useInboxPlanBreakdowns,
  useInboxScan,
  useInboxList,
  useInboxItemMetadata,
  useConeSearchSuggestions,
} from './queries';
export type {
  InboxBreakdownTarget,
  InboxItemMetadataState,
  InboxFileMetadata,
  InboxClassifyResponse,
  InboxListItem,
  InboxSourceGroupListItem,
  InboxListResponse,
  InboxScanFolderResponse,
  ConeSearchSuggestResponse,
  ConeSearchSuggestion,
  ConeSearchCandidateTarget,
  ConeSearchConfidence,
  ConeSearchReason,
  PointingSource,
} from './queries';

export {
  normalizeConfirmError,
  useInboxConfirm,
  useInboxClassifySourceGroup,
  useInboxReclassify,
} from './confirm';
export type {
  ConfirmError,
  ConfirmState,
  ClassifySourceGroupState,
  ReclassifyState,
  InboxConfirmResponse,
  InboxReclassifyResponse,
} from './confirm';

export {
  useInboxPlan,
  useInboxPlanApplyAll,
  useInboxPlanCancel,
  useOpenInboxPlans,
  useApplySelectedInboxPlans,
  useInboxStats,
} from './plans';
export type {
  OpenPlansState,
  InboxPlanView,
  InboxOpenPlan,
  InboxOpenPlansResponse,
  InboxPlanAction,
  PlanApplyResponse,
  InboxApplyAllResponse,
  InboxPlanCancelResponse,
  InboxStatsResponse,
  InboxStatsPerType,
  InboxStatsTotals,
} from './plans';

export {
  mergeRescanRoots,
  useInboxRescan,
  useConeSearchConfirm,
} from './scan';
export type {
  RescanState,
  RescanRoot,
  ConeSearchConfirmState,
} from './scan';
