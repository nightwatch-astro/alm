// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

export { ListDetailLayout } from './ListDetailLayout';
export type { ListDetailLayoutProps } from './ListDetailLayout';
export { PageShell } from './PageShell';
export type { PageShellProps } from './PageShell';

// spec 043 shared layout system (tasks #62/#63/#73): the Sessions page pattern
// generalized — pinned top bar + prop-driven filter row + list/detail scaffold.
export { PageTopBar } from './PageTopBar';
export type { PageTopBarProps } from './PageTopBar';
export { FilterToolbar } from './FilterToolbar';
export type {
  FilterToolbarProps,
  FilterField,
  MultiFilterField,
  FilterOption,
  GroupByControl,
  GroupingControl,
  SortControl,
  SearchControl,
} from './FilterToolbar';
export { ListPageLayout } from './ListPageLayout';
export type { ListPageLayoutProps } from './ListPageLayout';
export { DetailDockPlacementControl } from './DetailDockPlacementControl';
export type {
  DetailDockPlacementControlProps,
  DetailDockMode,
} from './DetailDockPlacementControl';
export { SortHeader, ariaSortFor } from './SortHeader';
export type { SortHeaderProps } from './SortHeader';
export { ListSidebar } from './ListSidebar';
export type { ListSidebarProps } from './ListSidebar';
export { ListItem } from './ListItem';
export type { ListItemProps } from './ListItem';
export { TopActionBar } from './TopActionBar';
export type { TopActionBarProps } from './TopActionBar';
export { DetailHeader } from './DetailHeader';
export type { DetailHeaderProps } from './DetailHeader';
export { DetailPane } from './DetailPane';
export type { DetailPaneProps } from './DetailPane';
// spec 043 §4 — tasks #100/#99/#101: shared detail panel + FactsKV helper.
export { DetailPanel, FactsKV } from './DetailPanel';
export type {
  DetailPanelProps,
  DetailPanelVariant,
  FactsKVProps,
} from './DetailPanel';

// Design v4 detail standard: identity header → metric line → dashboard grid
// (primary column + unified rail panel), plus the centralized lifecycle.
export { MetricLine } from './MetricLine';
export type { Metric, MetricLineProps } from './MetricLine';

// #813: the shared two-col-properties + linked-entity detail recipe
// (`.pv-session-detail2`), wrapped once instead of hand-copied per feature.
export { TwoColDetailLayout, DetailLinkedGroup } from './TwoColDetailLayout';
export type {
  TwoColDetailLayoutProps,
  DetailLinkedGroupProps,
} from './TwoColDetailLayout';
export { Lifecycle } from './Lifecycle';
export type { LifecycleProps } from './Lifecycle';

export { Modal } from './Modal';
export type { ModalProps, ModalSize } from './Modal';

// C-13: shared confirm dialog for delete/disable actions.
export { ConfirmModal } from './ConfirmModal';
export type {
  ConfirmModalProps,
  ConfirmModalActionVariant,
} from './ConfirmModal';

// C-28: shared loading/error/empty gate for list pages.
export { TableStateGate } from './TableStateGate';
export type { TableStateGateProps } from './TableStateGate';

// Shared status indicator (dot + label).
export { StatusTag } from './StatusTag';
export type { StatusTagProps } from './StatusTag';

// spec 035: SIMBAD target resolution — project-creation target typeahead.
export { TargetSearch } from './TargetSearch';
export type { TargetSearchProps } from './TargetSearch';

// PropertyTable is retained for feature files that still import it directly.
// Migrate callers to inline prop-table markup with pv-* CSS classes over time.
export { PropertyTable } from './PropertyTable';
export type { PropertyDef, PropertyTableProps } from './PropertyTable';

// spec-030 Q16 (#620, #619): shared missing-value renderer — real value
// (+source pill) / unresolved chip / not-applicable blank, driven by an
// explicit applicability marker (never inferred from data absence).
export {
  renderValue,
  renderValueOnly,
  valueState,
  UnresolvedChip,
  SourceBadge,
  NOT_APPLICABLE_DISPLAY,
} from './RenderValue';
export type {
  FieldApplicability,
  ValueSource,
  ValueState,
  RenderValueOptions,
} from './RenderValue';

// Legacy ListSidebar types re-exported for feature files pending migration.
export type {
  SelectOption,
  FilterPill,
  DropdownDef,
  ActionDef,
} from './legacy-types';
