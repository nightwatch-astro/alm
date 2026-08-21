// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Operation state persistence model and repository contract.
//! Ported from rusqlite to sqlx; model structs preserved for compatibility.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::DbResult;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OperationState {
    pub id: String,
    pub operation_type: OperationType,
    pub status: OperationStateStatus,
    pub progress: OperationProgress,
    pub current_message: Option<String>,
    pub started_at: Option<OffsetDateTime>,
    pub finished_at: Option<OffsetDateTime>,
    pub resume_token: Option<String>,
    pub error: Option<OperationStateError>,
    pub updated_at: OffsetDateTime,
}

impl OperationState {
    #[must_use]
    pub fn queued(
        id: impl Into<String>,
        operation_type: OperationType,
        now: OffsetDateTime,
    ) -> Self {
        Self {
            id: id.into(),
            operation_type,
            status: OperationStateStatus::Queued,
            progress: OperationProgress::default(),
            current_message: None,
            started_at: None,
            finished_at: None,
            resume_token: None,
            error: None,
            updated_at: now,
        }
    }

    pub fn mark_running(&mut self, now: OffsetDateTime, message: Option<String>) {
        self.status = OperationStateStatus::Running;
        self.started_at = Some(now);
        self.current_message = message;
        self.updated_at = now;
    }

    pub fn mark_failed(&mut self, now: OffsetDateTime, error: OperationStateError) {
        self.status = OperationStateStatus::Failed;
        self.finished_at = Some(now);
        self.error = Some(error);
        self.updated_at = now;
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationType {
    ScanRoot,
    ExtractMetadata,
    Classify,
    MatchCalibration,
    GeneratePlan,
    ApplyPlan,
    ObserveWorkspace,
    GenerateManifest,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStateStatus {
    Queued,
    Running,
    Pausing,
    Paused,
    Cancelling,
    Cancelled,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct OperationProgress {
    pub current: Option<u64>,
    pub total: Option<u64>,
}

impl OperationProgress {
    #[must_use]
    pub const fn new(current: Option<u64>, total: Option<u64>) -> Self {
        Self { current, total }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OperationStateError {
    pub code: String,
    pub message: String,
}

impl OperationStateError {
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }
}

/// Repository trait for operation state. Implementors use `sqlx::SqlitePool`.
pub trait OperationStateRepository {
    fn insert_operation_state(
        &mut self,
        state: &OperationState,
    ) -> impl std::future::Future<Output = DbResult<()>> + Send;

    fn update_operation_state(
        &mut self,
        state: &OperationState,
    ) -> impl std::future::Future<Output = DbResult<()>> + Send;

    fn find_operation_state(
        &self,
        id: &str,
    ) -> impl std::future::Future<Output = DbResult<Option<OperationState>>> + Send;
}

// ── In-memory implementation for tests ───────────────────────────────────────

#[derive(Default)]
pub struct InMemoryOperationStateRepository {
    states: std::collections::BTreeMap<String, OperationState>,
}

// `async` here is the trait's contract, not this double's need: the real SQLite
// implementation awaits, a `BTreeMap` has nothing to await. Returning
// `std::future::ready` instead, as the lint suggests, would run each body at call
// time rather than at poll time, which is a behaviour change for a test double.
//
// `allow` rather than `expect`, and paired with `unknown_lints`, because clippy
// added `unused_async_trait_impl` in 1.98: an expectation would go unfulfilled on
// older toolchains, and naming the lint at all is itself unknown to them.
#[allow(unknown_lints, clippy::unused_async_trait_impl)]
impl OperationStateRepository for InMemoryOperationStateRepository {
    async fn insert_operation_state(&mut self, state: &OperationState) -> DbResult<()> {
        self.states.insert(state.id.clone(), state.clone());
        Ok(())
    }

    async fn update_operation_state(&mut self, state: &OperationState) -> DbResult<()> {
        self.states.insert(state.id.clone(), state.clone());
        Ok(())
    }

    async fn find_operation_state(&self, id: &str) -> DbResult<Option<OperationState>> {
        Ok(self.states.get(id).cloned())
    }
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use super::{
        InMemoryOperationStateRepository, OperationState, OperationStateError,
        OperationStateRepository, OperationStateStatus, OperationType,
    };

    #[tokio::test]
    async fn creates_queued_operation_state() {
        let now = OffsetDateTime::UNIX_EPOCH;
        let state = OperationState::queued("op-1", OperationType::ScanRoot, now);

        assert_eq!(state.status, OperationStateStatus::Queued);
        assert_eq!(state.operation_type, OperationType::ScanRoot);
        assert_eq!(state.updated_at, now);
    }

    #[tokio::test]
    async fn records_running_and_failed_transitions() {
        let start = OffsetDateTime::UNIX_EPOCH;
        let finish = start + time::Duration::seconds(10);
        let mut state = OperationState::queued("op-1", OperationType::ApplyPlan, start);

        state.mark_running(start, Some("Applying plan".to_owned()));
        assert_eq!(state.status, OperationStateStatus::Running);
        assert_eq!(state.started_at, Some(start));

        state
            .mark_failed(finish, OperationStateError::new("plan.item_failed", "Plan item failed."));
        assert_eq!(state.status, OperationStateStatus::Failed);
        assert_eq!(state.finished_at, Some(finish));
        assert_eq!(state.error.unwrap().code, "plan.item_failed");
    }

    #[tokio::test]
    async fn repository_contract_supports_insert_update_and_lookup() {
        let mut repo = InMemoryOperationStateRepository::default();
        let mut state = OperationState::queued(
            "op-1",
            OperationType::GenerateManifest,
            OffsetDateTime::UNIX_EPOCH,
        );

        repo.insert_operation_state(&state).await.unwrap();
        state.mark_running(OffsetDateTime::UNIX_EPOCH, Some("Generating manifest".to_owned()));
        repo.update_operation_state(&state).await.unwrap();

        assert_eq!(
            repo.find_operation_state("op-1").await.unwrap().unwrap().status,
            OperationStateStatus::Running
        );
    }
}
