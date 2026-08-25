// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

export type {
  AlmClient,
  AlmClientOptions,
  AlmCancellationSignal,
  AlmTransport,
  ExecuteOperationOptions,
  ResponseEnvelope,
  SubscribeOperationOptions,
  TypedOkResponseEnvelope,
  TypedRequestEnvelope,
} from "./client";
export { AlmContractError } from "./client";

export type * from "./generated/envelope";

// Spec 002 — Data Lifecycle State Model. Namespaced re-export because the
// `lifecycle.transition` and `provenance.read` schemas share generic names
// (`Request`, `Response`, `ErrorCode`, `Timestamp`, `Uuid`, …) at the top
// level after json2ts; the namespace keeps callers explicit.
export type * as LifecycleTransition from "./generated/lifecycle.transition";
export type * as ProvenanceRead from "./generated/provenance.read";

// Spec 004 — Native Filesystem Controls.
export type * as NativeDirectoryPick from "./generated/native.directory.pick";
export type * as NativeFilePick from "./generated/native.file.pick";
export type * as NativeReveal from "./generated/native.reveal";

// Spec 013 — Target Lookup From FITS OBJECT.
export type * as TargetLookup from "./generated/target.lookup";
export type * as TargetResolve from "./generated/target.resolve";

// Spec 006 — Inventory Lifecycle.
export type * as InventoryList from "./generated/inventory.list";
export type * as InventorySessionReview from "./generated/inventory.session.review";

// Spec 012 — Processing Artifact Observation.
export type * as ArtifactList from "./generated/artifact.list";
export type * as ArtifactClassify from "./generated/artifact.classify";
export type * as WorkflowRunCompleted from "./generated/workflow.run_completed";

// Spec 022 — Desktop theme mode (theme.get / theme.set).
export type * as ThemeGet from "./generated/theme.get";
export type * as ThemeSet from "./generated/theme.set";

// Spec 019 — Bottom Log Viewer.
export type * as LogStream from "./generated/log.stream";
export type * as LogExport from "./generated/log.export";
