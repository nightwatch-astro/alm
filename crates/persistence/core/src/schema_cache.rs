// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Embedded database migrations.
//!
//! Before 1.0 the schema is a single editable baseline. Make schema changes
//! directly in `0001_initial_schema.sql` rather than adding a `0002+` migration;
//! every such edit requires existing development databases to be recreated.

/// The migration set consumed by [`crate::Database`].
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
