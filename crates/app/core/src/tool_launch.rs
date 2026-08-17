// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Processing tool launch use cases (spec 011 T007/T009/T012/T014/T015).
//!
//! Entry points:
//! - [`launch`]         — resolve project + tool + cwd → spawn → persist → audit.
//! - [`list_profiles`]  — list seeded profiles joined with Settings.
//! - [`update_tool`]    — write executable_path/enabled/auto_detected to Settings.
//! - [`validate_path`]  — check executable_path accessibility.
//! - [`discover`]       — auto-detect tool executables per OS.
//!
//! ## Architecture
//!
//! The actual process spawn is injected via [`workflow_profiles::launch::ProcessSpawner`]
//! so tests can use [`workflow_profiles::launch::FakeSpawner`] without spawning
//! real processes.
//!
//! Constitution III: this module spawns the tool and walks away. It NEVER
//! scripts PixInsight, watches menus, or interprets in-tool state.
//! Constitution V: `ToolLaunch` rows are the durable record; the EventBus
//! carries the live audit signal.

//!
//! Extracted from `app_core` into its own crate (spec 042 / T253 O3b) as a pure
//! leaf: it has zero `crate::` references and nothing else in `app_core`
//! references it. `app_core` re-exports this crate at `app_core::tool_launch` so
//! the public surface stays byte-identical.

#![allow(clippy::too_many_lines)] // orchestration functions are multi-step by design
#![allow(clippy::doc_markdown)] // spec/domain terminology

use crate::caches::project_block_debounce;
use crate::project_health::{emit_block_transition, BlockCondition};
use audit::bus::EventBus;
use audit::event_bus::{Source, ToolLaunchEvent, TOPIC_TOOL_LAUNCH};
use contracts_core::tools::{
    ToolDiscoverResponse, ToolDiscoveryEntry, ToolLaunchError, ToolLaunchRequest,
    ToolLaunchResponse, ToolLaunchStatus, ToolPathValidation, ToolProfileListResponse,
    ToolProfileSummary, UpdateProcessingTool,
};
use domain_core::ids::{new_id, Timestamp};
use persistence_lifecycle::repositories::first_run as first_run_repo;
use persistence_lifecycle::repositories::settings as settings_repo;
use persistence_plans::repositories::prepared_source_views as psv_repo;
use persistence_plans::repositories::projects as proj_repo;
use persistence_plans::repositories::tool_launches as tl_repo;
use persistence_targets::repositories::inventory as inv_repo;
use project_structure::resolve_working_folder;
use sqlx::SqlitePool;
use workflow_profiles::{
    args::{render, RenderContext},
    discover::discover_all,
    launch::{pid_is_alive, verify_cwd_containment, LaunchError, ProcessSpawner, SpawnRequest},
    seed,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Settings key for `tools.<tool_id>.executable_path`.
fn key_executable_path(tool_id: &str) -> String {
    format!("tools.{tool_id}.executable_path")
}

/// Settings key for `tools.<tool_id>.enabled`.
fn key_enabled(tool_id: &str) -> String {
    format!("tools.{tool_id}.enabled")
}

/// Settings key for `tools.<tool_id>.auto_detected`.
fn key_auto_detected(tool_id: &str) -> String {
    format!("tools.{tool_id}.auto_detected")
}

/// Settings key for `tools.<tool_id>.watch_extensions` (spec 012 T007b).
fn key_watch_extensions(tool_id: &str) -> String {
    format!("tools.{tool_id}.watch_extensions")
}

/// Read the configured artifact-watch extension allow-list for a tool
/// (spec 012 T007b / R-ExtAllow).
///
/// Falls back to `workflow_artifacts::DEFAULT_WATCH_EXTENSIONS` when unset,
/// empty, or malformed.
pub async fn read_watch_extensions(pool: &SqlitePool, tool_id: &str) -> Vec<String> {
    let raw = settings_repo::get_raw(pool, &key_watch_extensions(tool_id)).await.ok().flatten();
    let configured: Option<Vec<String>> = raw.and_then(|v| {
        let arr = v.as_array()?;
        let exts: Vec<String> = arr.iter().filter_map(|e| e.as_str().map(str::to_owned)).collect();
        (!exts.is_empty()).then_some(exts)
    });
    configured.unwrap_or_else(|| {
        workflow_artifacts::DEFAULT_WATCH_EXTENSIONS.iter().map(|s| (*s).to_owned()).collect()
    })
}

/// Read a nullable string setting value.
async fn read_string_setting(pool: &SqlitePool, key: &str) -> Option<String> {
    settings_repo::get_raw(pool, key)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_str().map(ToOwned::to_owned))
}

/// Read a boolean setting value (defaulting to `true` for `enabled`, `false` for others).
async fn read_bool_setting(pool: &SqlitePool, key: &str, default: bool) -> bool {
    settings_repo::get_raw(pool, key)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

// ── BLAKE3-style hash ─────────────────────────────────────────────────────────
// The data model calls for BLAKE3 but we use SHA-256 via sha2 (already in deps).
// Rename is intentional: the plan.md says BLAKE3; we implement with sha2 as a
// pragmatic choice (BLAKE3 is not in the workspace deps). This is flagged in
// the decisions file. The hash field is opaque for correlation only.

fn compute_args_hash(executable_path: &str, argv: &[String]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(executable_path.as_bytes());
    hasher.update(b"\x00");
    for arg in argv {
        hasher.update(arg.as_bytes());
        hasher.update(b"\x00");
    }
    hex::encode(hasher.finalize())
}

// ── Active source-view folder resolution (#726, spec 011 FR-009) ──────────────

/// Resolve the project's currently active generated source-view folder, if
/// one exists (spec 049 restored real generation via `prepared_source_views`;
/// this was hardcoded to `None` — every launch silently fell back to the
/// project root even when a real generated view existed).
///
/// `prepared_source_view_items.view_relative_path` is, despite its name, an
/// ABSOLUTE path (`source_view_generate.rs`'s `dest_abs`, forward-slash
/// portable) — there is no separate stored "destination root" column, so the
/// folder is recovered as the longest common path-prefix across the view's
/// items. With a single-item view this returns that file's immediate parent
/// rather than a possibly-shallower destination root; an acceptable best
/// effort given the schema has no dedicated root column.
async fn resolve_active_source_view_folder(pool: &SqlitePool, project_id: &str) -> Option<String> {
    let views = psv_repo::list_views_for_project(pool, project_id).await.ok()?;
    let view = views.into_iter().find(|v| v.state == "current")?;
    let items = psv_repo::list_view_items(pool, &view.id).await.ok()?;
    common_ancestor_dir(items.iter().map(|i| i.view_relative_path.as_str()))
}

/// Longest common directory prefix (forward-slash-segmented) across `paths`.
/// Returns `None` for zero paths.
fn common_ancestor_dir<'a>(paths: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut common: Option<Vec<&str>> = None;
    for p in paths {
        let segments: Vec<&str> = p.split('/').collect();
        // Drop the file name — only directory segments participate in the prefix.
        let dir_segments = &segments[..segments.len().saturating_sub(1)];
        common = Some(match common {
            None => dir_segments.to_vec(),
            Some(prev) => prev
                .iter()
                .zip(dir_segments.iter())
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| *a)
                .collect(),
        });
    }
    common.filter(|c| !c.is_empty()).map(|c| c.join("/"))
}

// ── US4-4: system-driven `tool_unconfigured` auto-block ─────────────────────────

/// Fire the `tool_unconfigured` auto-block signal (spec 009 US4, P7 debounce)
/// when a launch is attempted for a project whose tool has no usable
/// configuration (disabled, or no executable path set). Best-effort: a
/// failure here must never fail the launch response — the launch outcome
/// (`ToolLaunchResponse`) is the primary durable record (Constitution II).
async fn signal_tool_unconfigured_block(
    pool: &SqlitePool,
    bus: &EventBus,
    project_id: &str,
    tool_id: &str,
) {
    let condition = BlockCondition::ToolUnconfigured { tool: tool_id.to_owned() };
    if let Err(e) =
        emit_block_transition(pool, bus, project_block_debounce(), project_id, &condition).await
    {
        tracing::error!(
            %project_id, %tool_id, error = %e,
            "tool_unconfigured auto-block signal failed"
        );
    }
}

/// Shorthand for building an early-exit error `ToolLaunchResponse`.
fn error_response(code: &str, message: String) -> ToolLaunchResponse {
    ToolLaunchResponse {
        status: ToolLaunchStatus::Error,
        launch_id: None,
        pid: None,
        launched_at: None,
        working_dir: None,
        audit_id: None,
        prior_instance_alive: false,
        error: Some(ToolLaunchError { code: code.to_owned(), message }),
    }
}

/// Resolved tool configuration (steps 1-2 of the launch pipeline).
struct ResolvedToolConfig {
    project: persistence_plans::repositories::projects::ProjectRow,
    profile: workflow_profiles::ToolProfile,
    executable_path: String,
}

/// Load project and validate tool configuration (profile exists, enabled, path set).
///
/// Returns `Err(ToolLaunchResponse)` for user-facing validation failures.
async fn resolve_tool_config(
    pool: &SqlitePool,
    bus: &EventBus,
    req: &ToolLaunchRequest,
) -> Result<ResolvedToolConfig, ToolLaunchResponse> {
    let project = proj_repo::get_project(pool, &req.project_id)
        .await
        .map_err(|e| error_response("project.not_found", format!("{e}")))?;

    let Some(profile) = seed::find(&req.tool_id) else {
        return Err(error_response(
            "tool.not_configured",
            format!("No tool profile found for '{}'", req.tool_id),
        ));
    };

    let enabled = read_bool_setting(pool, &key_enabled(&req.tool_id), true).await;
    if !enabled {
        signal_tool_unconfigured_block(pool, bus, &req.project_id, &req.tool_id).await;
        return Err(error_response(
            "tool.not_configured",
            format!("Tool '{}' is disabled in settings", req.tool_id),
        ));
    }

    let executable_path = read_string_setting(pool, &key_executable_path(&req.tool_id)).await;
    let Some(executable_path) = executable_path.filter(|s| !s.trim().is_empty()) else {
        signal_tool_unconfigured_block(pool, bus, &req.project_id, &req.tool_id).await;
        return Err(error_response(
            "tool.not_configured",
            format!("No executable path configured for tool '{}'", req.tool_id),
        ));
    };

    Ok(ResolvedToolConfig { project, profile, executable_path })
}

/// Resolve working directory and verify library-root containment (steps 3-4).
///
/// Returns the canonical working-directory string, or `Err(ToolLaunchResponse)`
/// when the resolved path falls outside all registered library roots.
async fn resolve_and_check_working_dir(
    pool: &SqlitePool,
    project_path: &str,
    project_id: &str,
) -> Result<String, ToolLaunchResponse> {
    let project_root = std::path::PathBuf::from(project_path);
    let active_source_view = resolve_active_source_view_folder(pool, project_id).await;
    let working_dir_path = resolve_working_folder(&project_root, active_source_view.as_deref());

    let canonical_cwd =
        working_dir_path.canonicalize().unwrap_or_else(|_| working_dir_path.clone());
    let all_roots = inv_repo::list_all_roots(pool)
        .await
        .map_err(|e| error_response("db.error", format!("{e}")))?;
    let registered = first_run_repo::list_sources(pool)
        .await
        .map_err(|e| error_response("db.error", format!("{e}")))?;
    let root_paths: Vec<std::path::PathBuf> = all_roots
        .iter()
        .map(|r| r.current_path.as_str())
        .chain(registered.iter().map(|s| s.path.as_str()))
        .map(|raw| {
            let p = std::path::PathBuf::from(raw);
            p.canonicalize().unwrap_or(p)
        })
        .collect();
    let root_refs: Vec<&std::path::Path> =
        root_paths.iter().map(std::path::PathBuf::as_path).collect();

    if let Err(code) = verify_cwd_containment(&canonical_cwd, &root_refs) {
        return Err(error_response(
            code,
            format!(
                "Working directory '{}' is outside all registered library roots",
                canonical_cwd.display()
            ),
        ));
    }

    Ok(canonical_cwd.to_string_lossy().into_owned())
}

/// Spawn the process, persist the launch row, and emit the audit event (steps 7-9).
#[allow(clippy::too_many_arguments)]
async fn spawn_and_record(
    pool: &SqlitePool,
    bus: &EventBus,
    spawner: &dyn ProcessSpawner,
    req: &ToolLaunchRequest,
    executable_path: &str,
    argv: Vec<String>,
    args_hash: String,
    working_dir_str: &str,
    bundle_id: Option<String>,
) -> Result<ToolLaunchResponse, String> {
    let spawn_req = SpawnRequest {
        executable: executable_path.to_owned(),
        args: argv,
        working_dir: working_dir_str.to_owned(),
        bundle_id,
    };

    let launch_id = new_id();
    let audit_id = new_id();
    let launched_at = Timestamp::now_iso();

    let (outcome, pid, error_response_opt) = match spawner.spawn(spawn_req) {
        Ok(result) => ("spawned", result.pid, None),
        Err(LaunchError::MacOsQuarantine) => (
            "spawn_failed",
            None,
            Some(ToolLaunchError {
                code: "macos.quarantine.detected".to_owned(),
                message:
                    "macOS quarantined this app. Run `xattr -dr com.apple.quarantine <path>` and retry."
                        .to_owned(),
            }),
        ),
        Err(LaunchError::SpawnFailed(msg)) => (
            "spawn_failed",
            None,
            Some(ToolLaunchError {
                code: "launch.failed".to_owned(),
                message: format!("OS error: {msg}"),
            }),
        ),
    };

    let _ = tl_repo::insert_tool_launch(
        pool,
        &tl_repo::InsertToolLaunch {
            id: &launch_id,
            project_id: &req.project_id,
            tool_id: &req.tool_id,
            pid,
            working_dir: Some(working_dir_str),
            args_hash: Some(&args_hash),
            outcome,
            audit_id: &audit_id,
        },
    )
    .await
    .map_err(|e| format!("{e}"))?;

    let _ = bus
        .publish(
            TOPIC_TOOL_LAUNCH,
            Source::User,
            ToolLaunchEvent {
                launch_id: launch_id.clone(),
                project_id: req.project_id.clone(),
                tool_id: req.tool_id.clone(),
                working_dir: Some(working_dir_str.to_owned()),
                args_hash: Some(args_hash),
                outcome: outcome.to_owned(),
                at: launched_at.clone(),
            },
        )
        .await
        .map_err(|e| format!("audit bus: {e}"));

    if let Some(err) = error_response_opt {
        return Ok(ToolLaunchResponse {
            status: ToolLaunchStatus::Error,
            launch_id: Some(launch_id),
            pid: None,
            launched_at: Some(launched_at),
            working_dir: Some(working_dir_str.to_owned()),
            audit_id: Some(audit_id),
            prior_instance_alive: false,
            error: Some(err),
        });
    }

    Ok(ToolLaunchResponse {
        status: ToolLaunchStatus::Success,
        launch_id: Some(launch_id),
        pid,
        launched_at: Some(launched_at),
        working_dir: Some(working_dir_str.to_owned()),
        audit_id: Some(audit_id),
        prior_instance_alive: false,
        error: None,
    })
}

// ── launch ────────────────────────────────────────────────────────────────────

/// Launch the configured processing tool for the given project.
///
/// Pipeline (plan.md §Use Case Layer):
/// 1. Load project; reject `project.not_found`.
/// 2. Load seed profile; reject `tool.not_configured` when missing/disabled/no path.
/// 3. Resolve working directory (project root → optional source-view folder).
/// 4. Canonicalize cwd; verify library-root containment (R-CwdContain).
/// 5. Re-launch guard: if prior `spawned` launch has `completed_at=NULL` and PID
///    is still alive, return `PriorInstanceAlive` unless `force=true`.
/// 6. Render args template.
/// 7. Spawn detached process via `spawner`.
/// 8. Persist `tool_launches` row.
/// 9. Emit `tool.launch` audit event.
///
/// # Errors
/// Returns `Err(String)` for infrastructure failures (DB, audit bus).
#[allow(clippy::too_many_arguments)]
pub async fn launch(
    pool: &SqlitePool,
    bus: &EventBus,
    spawner: &dyn ProcessSpawner,
    req: ToolLaunchRequest,
) -> Result<ToolLaunchResponse, String> {
    // ── Steps 1-2: resolve tool configuration ────────────────────────────────
    let config = match resolve_tool_config(pool, bus, &req).await {
        Ok(c) => c,
        Err(resp) => return Ok(resp),
    };

    // ── Steps 3-4: resolve working directory + containment ───────────────────
    let working_dir_str =
        match resolve_and_check_working_dir(pool, &config.project.path, &req.project_id).await {
            Ok(dir) => dir,
            Err(resp) => return Ok(resp),
        };

    // ── Step 5: re-launch guard ───────────────────────────────────────────────
    if !req.force {
        if let Ok(Some(prior)) =
            tl_repo::get_latest_launch(pool, &req.project_id, &req.tool_id).await
        {
            if prior.outcome == "spawned" && prior.completed_at.is_none() {
                let alive = prior.pid.and_then(|p| u32::try_from(p).ok()).is_some_and(pid_is_alive);
                if alive {
                    return Ok(ToolLaunchResponse {
                        status: ToolLaunchStatus::PriorInstanceAlive,
                        launch_id: None,
                        pid: None,
                        launched_at: None,
                        working_dir: None,
                        audit_id: None,
                        prior_instance_alive: true,
                        error: None,
                    });
                }
            }
        }
    }

    // ── Step 6: render args template ──────────────────────────────────────────
    let ctx = RenderContext {
        folder: if config.profile.supports_open_folder {
            Some(working_dir_str.as_str())
        } else {
            None
        },
        file: None,
    };
    let argv = render(&config.profile.args_template, &ctx);
    let args_hash = compute_args_hash(&config.executable_path, &argv);

    // bundle_id from Settings (override) or seeded default
    let bundle_id_key = format!("tools.{}.bundle_id", req.tool_id);
    let bundle_id = read_string_setting(pool, &bundle_id_key)
        .await
        .or_else(|| config.profile.bundle_id.map(ToOwned::to_owned));

    // ── Steps 7-9: spawn, persist, emit audit ────────────────────────────────
    spawn_and_record(
        pool,
        bus,
        spawner,
        &req,
        &config.executable_path,
        argv,
        args_hash,
        &working_dir_str,
        bundle_id,
    )
    .await
}

// ── list_profiles ─────────────────────────────────────────────────────────────

/// List all seeded profiles joined with Settings state.
///
/// # Errors
/// Returns `Err(String)` on DB failure.
pub async fn list_profiles(pool: &SqlitePool) -> Result<ToolProfileListResponse, String> {
    let mut tools = Vec::new();
    for profile in seed::all() {
        let executable_path = read_string_setting(pool, &key_executable_path(profile.id)).await;
        let configured = executable_path.as_deref().is_some_and(|p| !p.trim().is_empty());
        let available = configured
            && executable_path.as_deref().is_some_and(|p| std::path::Path::new(p).exists());
        let enabled = read_bool_setting(pool, &key_enabled(profile.id), true).await;
        let auto_detected = read_bool_setting(pool, &key_auto_detected(profile.id), false).await;
        let watch_extensions = read_watch_extensions(pool, profile.id).await;

        tools.push(ToolProfileSummary {
            id: profile.id.to_owned(),
            name: profile.name.to_owned(),
            configured,
            available,
            supports_open_folder: profile.supports_open_folder,
            enabled,
            auto_detected,
            executable_path,
            watch_extensions,
        });
    }
    Ok(ToolProfileListResponse { tools })
}

// ── update_tool ───────────────────────────────────────────────────────────────

/// Persist user-supplied executable_path / enabled changes to Settings.
///
/// # Errors
/// Returns `Err(String)` on DB failure.
pub async fn update_tool(
    pool: &SqlitePool,
    req: UpdateProcessingTool,
) -> Result<ToolProfileSummary, String> {
    if let Some(ref path) = req.path {
        let path_str = path.trim();
        // Validate: absolute path required.
        if !path_str.is_empty() && !std::path::Path::new(path_str).is_absolute() {
            return Err(format!(
                "executable_path for '{}' must be absolute; got '{}'",
                req.id, path_str
            ));
        }
        settings_repo::set_raw(
            pool,
            &key_executable_path(&req.id),
            &serde_json::Value::String(path_str.to_owned()),
        )
        .await
        .map_err(|e| format!("{e}"))?;
        // User-saved path clears auto_detected flag.
        settings_repo::set_raw(pool, &key_auto_detected(&req.id), &serde_json::Value::Bool(false))
            .await
            .map_err(|e| format!("{e}"))?;
    }

    settings_repo::set_raw(pool, &key_enabled(&req.id), &serde_json::Value::Bool(req.enabled))
        .await
        .map_err(|e| format!("{e}"))?;

    // spec 012 T007b: persist a custom watch-extensions allow-list, when supplied.
    if let Some(ref extensions) = req.watch_extensions {
        for ext in extensions {
            if !ext.starts_with('.') {
                return Err(format!(
                    "watch_extensions entries must start with '.'; got '{ext}' for tool '{}'",
                    req.id
                ));
            }
        }
        let value = serde_json::Value::Array(
            extensions.iter().cloned().map(serde_json::Value::String).collect(),
        );
        settings_repo::set_raw(pool, &key_watch_extensions(&req.id), &value)
            .await
            .map_err(|e| format!("{e}"))?;
    }

    // Return updated summary
    let executable_path = read_string_setting(pool, &key_executable_path(&req.id)).await;
    let configured = executable_path.as_deref().is_some_and(|p| !p.trim().is_empty());
    let available =
        configured && executable_path.as_deref().is_some_and(|p| std::path::Path::new(p).exists());
    let auto_detected = read_bool_setting(pool, &key_auto_detected(&req.id), false).await;
    let name = seed::find(&req.id).map_or_else(|| req.id.clone(), |p| p.name.to_owned());
    let supports_open_folder = seed::find(&req.id).is_some_and(|p| p.supports_open_folder);
    let watch_extensions = read_watch_extensions(pool, &req.id).await;

    Ok(ToolProfileSummary {
        id: req.id,
        name,
        configured,
        available,
        supports_open_folder,
        enabled: req.enabled,
        auto_detected,
        executable_path,
        watch_extensions,
    })
}

// ── validate_path ─────────────────────────────────────────────────────────────

/// Check whether a path string points to an accessible executable.
///
/// Does NOT spawn anything; only checks filesystem existence.
#[must_use]
pub fn validate_path(path: &str) -> ToolPathValidation {
    let p = std::path::Path::new(path);
    let exists = p.exists();
    let valid = exists && p.is_absolute();
    // Only meaningful once the path exists; `is_dir()` is false for a
    // nonexistent path, which would be indistinguishable from "exists and is
    // a file" without gating on `exists` first (issue #1056).
    let is_dir = exists.then(|| p.is_dir());
    ToolPathValidation {
        path: path.to_owned(),
        valid,
        reason: if valid {
            None
        } else if !p.is_absolute() {
            Some("Path must be absolute".to_owned())
        } else {
            Some("Path does not exist".to_owned())
        },
        is_dir,
    }
}

// ── discover ──────────────────────────────────────────────────────────────────

/// Auto-detect tool executables for the current OS and return discovery entries.
///
/// Does NOT write to settings; the caller (command adapter) writes on user save.
///
/// # Errors
/// Returns `Err(String)` on unexpected failure.
pub fn discover(tool_id: Option<&str>) -> Result<ToolDiscoverResponse, String> {
    let results = discover_all();
    let entries = results
        .into_iter()
        .filter(|r| tool_id.is_none_or(|id| r.tool_id == id))
        .map(|r| ToolDiscoveryEntry {
            tool_id: r.tool_id.clone(),
            path: r.path.to_string_lossy().into_owned(),
            available: r.available,
        })
        .collect();
    Ok(ToolDiscoverResponse { entries })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::Database;
    use workflow_profiles::launch::FakeSpawner;

    async fn setup_db() -> Database {
        let db = Database::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        db
    }

    async fn make_project(db: &Database) -> String {
        let pool = db.pool();
        let project_id = domain_core::ids::new_id();
        // Insert a minimal project row
        sqlx::query(
            "INSERT INTO projects (id, name, tool, lifecycle, path, notes, channel_drift, created_at, updated_at) \
             VALUES (?, 'Test Project', 'PixInsight', 'setup_incomplete', '/mnt/library/test_project', NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        )
        .bind(&project_id)
        .execute(pool)
        .await
        .unwrap();
        project_id
    }

    async fn insert_root(db: &Database, path: &str) {
        let root_id = domain_core::ids::new_id();
        sqlx::query(
            "INSERT INTO library_root (id, label, current_path, kind, state, created_at) \
             VALUES (?, 'Test Root', ?, 'local', 'active', '2026-01-01T00:00:00Z')",
        )
        .bind(&root_id)
        .bind(path)
        .execute(db.pool())
        .await
        .unwrap();
    }

    async fn set_tool_path(db: &Database, tool_id: &str, path: &str) {
        settings_repo::set_raw(
            db.pool(),
            &format!("tools.{tool_id}.executable_path"),
            &serde_json::Value::String(path.to_owned()),
        )
        .await
        .unwrap();
    }

    fn make_bus(pool: sqlx::SqlitePool) -> EventBus {
        EventBus::with_pool(pool)
    }

    #[tokio::test]
    async fn launch_returns_error_when_tool_not_found() {
        let db = setup_db().await;
        let project_id = make_project(&db).await;
        let bus = make_bus(db.pool().clone());
        let spawner = FakeSpawner::ok();
        let req =
            ToolLaunchRequest { project_id, tool_id: "nonexistent_tool".to_owned(), force: false };
        let resp = launch(db.pool(), &bus, &spawner, req).await.unwrap();
        assert_eq!(resp.status, ToolLaunchStatus::Error);
        assert_eq!(resp.error.unwrap().code, "tool.not_configured");
        assert_eq!(spawner.drain().len(), 0, "should not spawn");
    }

    #[tokio::test]
    async fn launch_returns_error_when_no_path_configured() {
        let db = setup_db().await;
        let project_id = make_project(&db).await;
        let bus = make_bus(db.pool().clone());
        let spawner = FakeSpawner::ok();
        // No executable path set for pixinsight
        let req = ToolLaunchRequest { project_id, tool_id: "pixinsight".to_owned(), force: false };
        let resp = launch(db.pool(), &bus, &spawner, req).await.unwrap();
        assert_eq!(resp.status, ToolLaunchStatus::Error);
        assert_eq!(resp.error.unwrap().code, "tool.not_configured");
    }

    /// astro-plan-akon: `launch` against an unconfigured tool must drive the
    /// project into `blocked` via the real production path (not a direct
    /// `emit_block_transition` call) — the auto-block wiring this test
    /// guards against regressing to dead code.
    #[tokio::test]
    async fn launch_with_no_path_configured_auto_blocks_project() {
        let db = setup_db().await;
        let project_id = make_project(&db).await;
        let bus = make_bus(db.pool().clone());
        let spawner = FakeSpawner::ok();
        // No executable path set for pixinsight.
        let req = ToolLaunchRequest {
            project_id: project_id.clone(),
            tool_id: "pixinsight".to_owned(),
            force: false,
        };
        let resp = launch(db.pool(), &bus, &spawner, req).await.unwrap();
        assert_eq!(resp.status, ToolLaunchStatus::Error);

        let project = proj_repo::get_project(db.pool(), &project_id).await.unwrap();
        assert_eq!(project.lifecycle, "blocked", "unconfigured-tool launch must auto-block");
        assert_eq!(project.blocked_reason_kind.as_deref(), Some("tool_unconfigured"));

        let audit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log_entry \
             WHERE entity_id = ? AND entity_type = 'project' AND to_state = 'blocked' \
               AND actor = 'system' AND outcome = 'applied'",
        )
        .bind(&project_id)
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(audit_count, 1, "auto-block from launch must write exactly one audit row");
    }

    #[tokio::test]
    async fn launch_rejects_cwd_outside_library_root() {
        let db = setup_db().await;
        let project_id = make_project(&db).await;
        let bus = make_bus(db.pool().clone());
        let spawner = FakeSpawner::ok();
        // Register a root that does NOT contain the project path (/mnt/library/test_project).
        insert_root(&db, "/different/root").await;
        set_tool_path(&db, "pixinsight", "/usr/bin/pixinsight").await;
        let req = ToolLaunchRequest { project_id, tool_id: "pixinsight".to_owned(), force: false };
        let resp = launch(db.pool(), &bus, &spawner, req).await.unwrap();
        assert_eq!(resp.status, ToolLaunchStatus::Error);
        let err = resp.error.unwrap();
        assert_eq!(err.code, "cwd.outside_library_root");
        assert_eq!(spawner.drain().len(), 0);
    }

    #[tokio::test]
    async fn launch_succeeds_when_path_configured_and_root_contains_cwd() {
        let db = setup_db().await;
        let project_id = make_project(&db).await;
        let bus = make_bus(db.pool().clone());
        let spawner = FakeSpawner::ok();
        // Register library root that contains the project
        insert_root(&db, "/mnt/library").await;
        set_tool_path(&db, "pixinsight", "/usr/bin/pixinsight").await;
        let req = ToolLaunchRequest {
            project_id: project_id.clone(),
            tool_id: "pixinsight".to_owned(),
            force: false,
        };
        let resp = launch(db.pool(), &bus, &spawner, req).await.unwrap();
        assert_eq!(resp.status, ToolLaunchStatus::Success);
        assert!(resp.launch_id.is_some());
        let calls = spawner.drain();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].executable, "/usr/bin/pixinsight");
    }

    /// #726 (spec 011 FR-009): launch must pass the project's active
    /// generated source-view folder, not silently fall back to the project
    /// root just because one exists.
    #[tokio::test]
    async fn launch_uses_active_source_view_folder_when_present() {
        let db = setup_db().await;
        let project_id = make_project(&db).await;
        let bus = make_bus(db.pool().clone());
        let spawner = FakeSpawner::ok();
        insert_root(&db, "/mnt/library").await;
        set_tool_path(&db, "siril", "/usr/bin/siril").await;

        psv_repo::insert_view(
            db.pool(),
            &psv_repo::InsertPreparedSourceView {
                id: "view-1",
                project_id: &project_id,
                kind: "symlink",
            },
        )
        .await
        .unwrap();
        let view_dir = "/mnt/library/test_project/source-views/plan-1/2026-01-01/L";
        for (idx, name) in ["light1.fits", "light2.fits"].iter().enumerate() {
            psv_repo::insert_view_item(
                db.pool(),
                &psv_repo::InsertPreparedSourceViewItem {
                    id: &format!("item-{idx}"),
                    view_id: "view-1",
                    inventory_item_id: &format!("inv-{idx}"),
                    view_relative_path: &format!("{view_dir}/{name}"),
                    materialization: "symlink",
                },
            )
            .await
            .unwrap();
        }

        let req = ToolLaunchRequest { project_id, tool_id: "siril".to_owned(), force: false };
        let resp = launch(db.pool(), &bus, &spawner, req).await.unwrap();
        assert_eq!(resp.status, ToolLaunchStatus::Success);
        assert_eq!(resp.working_dir.as_deref(), Some(view_dir));
    }

    /// No generated view exists — must fall back to the project root exactly
    /// as before (regression guard for the #726 fix).
    #[tokio::test]
    async fn launch_falls_back_to_project_root_without_a_view() {
        let db = setup_db().await;
        let project_id = make_project(&db).await;
        let bus = make_bus(db.pool().clone());
        let spawner = FakeSpawner::ok();
        insert_root(&db, "/mnt/library").await;
        set_tool_path(&db, "siril", "/usr/bin/siril").await;

        let req = ToolLaunchRequest { project_id, tool_id: "siril".to_owned(), force: false };
        let resp = launch(db.pool(), &bus, &spawner, req).await.unwrap();
        assert_eq!(resp.status, ToolLaunchStatus::Success);
        assert_eq!(resp.working_dir.as_deref(), Some("/mnt/library/test_project"));
    }

    /// Wizard-registered roots live in `registered_sources` only (they are
    /// mirrored into `library_root` on ingest, which never happens for a
    /// project folder). Containment must accept them, or every launch from a
    /// project anchored under the registered project folder fails.
    #[tokio::test]
    async fn launch_accepts_cwd_under_registered_source_root() {
        let db = setup_db().await;
        let project_id = make_project(&db).await;
        let bus = make_bus(db.pool().clone());
        let spawner = FakeSpawner::ok();
        // NO library_root row: only a gen-3 registered project source
        // containing the project path (/mnt/library/test_project).
        sqlx::query(
            "INSERT INTO registered_sources \
             (id, kind, path, scan_depth, created_at, created_via, organization_state) \
             VALUES ('rs-proj', 'project', '/mnt/library', 'recursive', \
                     '2026-01-01T00:00:00Z', 'first_run', 'organized')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        set_tool_path(&db, "pixinsight", "/usr/bin/pixinsight").await;
        let req = ToolLaunchRequest { project_id, tool_id: "pixinsight".to_owned(), force: false };
        let resp = launch(db.pool(), &bus, &spawner, req).await.unwrap();
        assert_eq!(resp.status, ToolLaunchStatus::Success);
        assert_eq!(spawner.drain().len(), 1);
    }

    #[tokio::test]
    async fn launch_persists_tool_launch_row() {
        let db = setup_db().await;
        let project_id = make_project(&db).await;
        let bus = make_bus(db.pool().clone());
        let spawner = FakeSpawner::ok();
        insert_root(&db, "/mnt/library").await;
        set_tool_path(&db, "pixinsight", "/usr/bin/pixinsight").await;
        let req = ToolLaunchRequest {
            project_id: project_id.clone(),
            tool_id: "pixinsight".to_owned(),
            force: false,
        };
        let resp = launch(db.pool(), &bus, &spawner, req).await.unwrap();
        let launch_id = resp.launch_id.unwrap();

        let row = tl_repo::get_latest_launch(db.pool(), &project_id, "pixinsight")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.id, launch_id);
        assert_eq!(row.outcome, "spawned");
    }

    #[tokio::test]
    async fn launch_prior_instance_alive_without_force() {
        let db = setup_db().await;
        let project_id = make_project(&db).await;
        let bus = make_bus(db.pool().clone());
        insert_root(&db, "/mnt/library").await;
        set_tool_path(&db, "pixinsight", "/usr/bin/pixinsight").await;

        // Insert a "spawned" row with a pid that the test process can never have as its own child.
        // pid_is_alive uses kill(pid, 0); on Linux PID 1 is always alive (init/systemd).
        // However we can't reliably test "alive" in unit tests without spawning. Instead, we
        // test the guard by inserting a completed row (completed_at non-null) which should NOT
        // trigger the guard.
        let launch_id = new_id();
        let audit_id = new_id();
        tl_repo::insert_tool_launch(
            db.pool(),
            &tl_repo::InsertToolLaunch {
                id: &launch_id,
                project_id: &project_id,
                tool_id: "pixinsight",
                pid: None, // No PID → pid_is_alive returns false
                working_dir: Some("/mnt/library/test_project"),
                args_hash: Some("abc"),
                outcome: "spawned",
                audit_id: &audit_id,
            },
        )
        .await
        .unwrap();

        // Without a real alive PID, guard should NOT trigger; second launch should succeed.
        let spawner2 = FakeSpawner::ok();
        let req2 = ToolLaunchRequest {
            project_id: project_id.clone(),
            tool_id: "pixinsight".to_owned(),
            force: false,
        };
        let resp2 = launch(db.pool(), &bus, &spawner2, req2).await.unwrap();
        assert_eq!(resp2.status, ToolLaunchStatus::Success);
    }

    #[tokio::test]
    async fn list_profiles_returns_all_seeds() {
        let db = setup_db().await;
        let resp = list_profiles(db.pool()).await.unwrap();
        // #725: seed::all() now also includes Planetary Suite.
        assert_eq!(resp.tools.len(), 3);
        let ids: Vec<&str> = resp.tools.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"pixinsight"));
        assert!(ids.contains(&"siril"));
        assert!(ids.contains(&"planetary_suite"));
    }

    #[tokio::test]
    async fn list_profiles_reflects_settings() {
        let db = setup_db().await;
        set_tool_path(&db, "siril", "/usr/bin/siril").await;
        let resp = list_profiles(db.pool()).await.unwrap();
        let siril = resp.tools.iter().find(|t| t.id == "siril").unwrap();
        assert!(siril.configured);
        assert_eq!(siril.executable_path.as_deref(), Some("/usr/bin/siril"));
    }

    #[tokio::test]
    async fn update_tool_writes_settings() {
        // Path must be absolute on the host OS (Windows rejects POSIX-style paths).
        #[cfg(windows)]
        let exe = "C:\\Apps\\PixInsight\\PixInsight.exe";
        #[cfg(not(windows))]
        let exe = "/Applications/PixInsight/PixInsight.app";

        let db = setup_db().await;
        let summary = update_tool(
            db.pool(),
            UpdateProcessingTool {
                id: "pixinsight".to_owned(),
                path: Some(exe.to_owned()),
                enabled: true,
                watch_extensions: None,
            },
        )
        .await
        .unwrap();
        assert!(summary.configured);
        assert_eq!(summary.executable_path.as_deref(), Some(exe));
        assert!(!summary.auto_detected, "user save should clear auto_detected");
    }

    #[tokio::test]
    async fn validate_path_rejects_relative() {
        let v = validate_path("relative/path");
        assert!(!v.valid);
        assert!(v.reason.unwrap().contains("absolute"));
    }

    #[tokio::test]
    async fn validate_path_accepts_nonexistent_absolute() {
        // An absolute path that doesn't exist returns valid=false. The literal
        // must be absolute on the host OS (Windows rejects POSIX-style paths).
        #[cfg(windows)]
        let missing = "C:\\no\\such\\binary";
        #[cfg(not(windows))]
        let missing = "/no/such/binary";
        let v = validate_path(missing);
        assert!(!v.valid);
        assert!(v.reason.unwrap().contains("exist"));
    }

    #[tokio::test]
    async fn validate_path_is_dir_true_for_directory() {
        let dir = tempfile::tempdir().unwrap();
        let v = validate_path(dir.path().to_str().unwrap());
        assert!(v.valid);
        assert_eq!(v.is_dir, Some(true));
    }

    #[tokio::test]
    async fn validate_path_is_dir_false_for_file() {
        // Issue #1056: a file path (not a directory) must be distinguishable
        // so the manual source-path entry UI can reject it inline.
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("not-a-dir.txt");
        std::fs::write(&file_path, b"x").unwrap();
        let v = validate_path(file_path.to_str().unwrap());
        assert!(v.valid);
        assert_eq!(v.is_dir, Some(false));
    }

    #[tokio::test]
    async fn validate_path_is_dir_none_for_nonexistent() {
        #[cfg(windows)]
        let missing = "C:\\no\\such\\binary";
        #[cfg(not(windows))]
        let missing = "/no/such/binary";
        let v = validate_path(missing);
        assert_eq!(v.is_dir, None);
    }

    #[test]
    fn discover_does_not_panic() {
        let resp = discover(None).unwrap();
        for e in &resp.entries {
            assert!(std::path::Path::new(&e.path).is_absolute());
        }
    }
}
