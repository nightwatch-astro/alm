// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Target contract DTOs for the Tauri IPC surface.
//!
//! ## Spec 029 legacy types (kept for spec-029 stub commands `targets.list`/`targets.get`)
//!
//! [`Target`], [`TargetDetail`], [`TargetProjectStub`], [`CatalogIds`],
//! [`Coordinates`], [`TargetKind`] — stub surface for the legacy list commands.
//!
//! ## Spec 036 gen-3 target management types
//!
//! [`TargetAliasDto`], [`TargetDetailV3`], [`TargetListItem`] — management
//! DTOs for `target.get`, `target.list`, `target.alias.add/remove`,
//! `target.display_alias.set/clear` (spec 036 / contracts/target-management.md).
//!
//! ## Spec 035 types (SIMBAD resolution)
//!
//! Search/resolve/settings DTOs for `target.search`, `target.resolve`,
//! `target.resolution.settings`.
//!
//! ## `target.astro_format.batch` (adopt target-match)
//!
//! [`TargetAstroFormatItem`], [`TargetAstroFormatBatchRequest`],
//! [`TargetAstroFormat`], [`TargetAstroFormatBatchResponse`] — batched
//! sexagesimal RA/Dec formatting for N targets in one IPC call.

use crate::lifecycle::ProjectState;
use crate::sessions::AcquisitionSession;
use serde::{Deserialize, Serialize};
use specta::Type;

// ── Spec 029 stub enums/structs (kept for targets.list / targets.get) ─────────

/// Classification of an astronomical target.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    DeepSky,
    Planetary,
    Lunar,
    Solar,
    Landscape,
}

/// Catalog identifiers for a target (NGC, IC, Messier, etc.).
#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CatalogIds {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ngc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messier: Option<String>,
}

/// Equatorial coordinates (J2000).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Coordinates {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ra: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dec: Option<f64>,
}

/// An astronomical target as seen in list views (spec 029 stub).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub catalog_ids: CatalogIds,
    pub kind: TargetKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinates: Option<Coordinates>,
    pub session_count: u32,
    pub project_count: u32,
    pub total_integration_hours: f64,
    /// Filter name -> acquired hours.
    pub coverage: std::collections::HashMap<String, f64>,
    /// Filter name -> recommended hours.
    pub recommended_hours: std::collections::HashMap<String, f64>,
}

/// A project stub within the target detail view (spec 029 stub).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetProjectStub {
    pub id: String,
    pub name: String,
    pub state: ProjectState,
}

/// Extended detail view of a target (spec 029 stub).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetDetail {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub catalog_ids: CatalogIds,
    pub kind: TargetKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinates: Option<Coordinates>,
    pub session_count: u32,
    pub project_count: u32,
    pub total_integration_hours: f64,
    pub coverage: std::collections::HashMap<String, f64>,
    pub recommended_hours: std::collections::HashMap<String, f64>,
    pub sessions: Vec<AcquisitionSession>,
    pub projects: Vec<TargetProjectStub>,
}

// ── Spec 036 gen-3 target management DTOs ─────────────────────────────────────
//
// Contracts per `specs/036-retire-legacy-targets/contracts/target-management.md`.

/// Kind of a target alias (gen-3).
///
/// - `"designation"` — a SIMBAD catalog designation (read-only, not removable).
/// - `"common_name"` — a SIMBAD curated common name (read-only, not removable).
/// - `"user"` — a user-added alias (removable via `target.alias.remove`).
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum AliasKind {
    Designation,
    CommonName,
    User,
}

/// A single alias row returned by `target.get` (gen-3).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetAliasDto {
    pub id: String,
    pub alias: String,
    pub kind: AliasKind,
}

/// Full target detail returned by `target.get` (gen-3).
///
/// `effectiveLabel` = `displayAlias ?? primaryDesignation`.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetDetailV3 {
    pub id: String,
    /// Canonical SIMBAD designation (read-only).
    pub primary_designation: String,
    /// User-set presentation label; `null` when not set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_alias: Option<String>,
    /// `displayAlias ?? primaryDesignation` — always non-null.
    pub effective_label: String,
    /// Closed object-type string (e.g. `"galaxy"`, `"emission_nebula"`).
    pub object_type: String,
    /// ICRS J2000 right ascension in decimal degrees.
    pub ra_deg: f64,
    /// ICRS J2000 declination in decimal degrees.
    pub dec_deg: f64,
    /// SIMBAD physical-object id (dedup key); `null` for seed/override entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub simbad_oid: Option<i64>,
    /// Provenance: `"seed"`, `"resolved"`, or `"user-override"`.
    pub source: String,
    /// All aliases (designations, common names, user-added).
    pub aliases: Vec<TargetAliasDto>,
}

/// A single row in the target list returned by `target.list` (gen-3).
///
/// `raDeg` and `decDeg` are always populated (sourced from `canonical_target`).
/// `constellation` and `magnitude` are optional because those columns were not
/// in the original schema; they are populated from `canonical_target.constellation`
/// and `canonical_target.magnitude` when present (migration 0046).
/// Alias search is now backend-side via the `search` parameter on `target.list`
/// (GF-11 / DS-16); `aliases` is no longer serialized to the IPC response to
/// avoid cloning all alias strings for every list call (13k× entries).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetListItem {
    pub id: String,
    pub effective_label: String,
    pub primary_designation: String,
    pub object_type: String,
    /// ICRS J2000 right ascension in decimal degrees.
    pub ra_deg: f64,
    /// ICRS J2000 declination in decimal degrees.
    pub dec_deg: f64,
    /// IAU constellation abbreviation (e.g. `"And"`, `"Ori"`); `null` when
    /// not yet stored (no constellation column in the schema before migration 0046).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constellation: Option<String>,
    /// Visual magnitude; `null` when not stored or not applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub magnitude: Option<f64>,
    /// Alias display forms — kept in-process for backend search but NOT
    /// serialized to the IPC response (detail-only; use `target.get` for the
    /// full alias list, or pass `search` to `target.list` for alias-aware
    /// filtering without cloning all alias strings over IPC).
    #[serde(skip)]
    pub aliases: Vec<String>,
    /// Count of `acquisition_session` rows linked to this target (#877, planner
    /// Sessions column). `0` when no session has resolved to this target yet —
    /// additive field, older clients ignoring unknown keys are unaffected.
    #[serde(default)]
    pub session_count: u32,
}

// ── Gen-3 request / response types ────────────────────────────────────────────

/// Request for `target.get` (gen-3).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetGetRequest {
    pub target_id: String,
}

/// Request for `target.alias.add` (gen-3).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetAliasAddRequest {
    pub target_id: String,
    /// User-supplied alias display form; server normalizes.
    pub alias: String,
}

/// Response for `target.alias.add` (gen-3).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetAliasAddResult {
    /// The newly created alias row.
    pub alias: TargetAliasDto,
}

/// Request for `target.alias.remove` (gen-3).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetAliasRemoveRequest {
    pub target_id: String,
    /// The `id` of the alias row to remove (only `kind=user` is removable).
    pub alias_id: String,
}

/// Response for `target.alias.remove` (gen-3).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetAliasRemoveResult {
    pub removed: bool,
}

/// Request for `target.display_alias.set` (gen-3).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetDisplayAliasSetRequest {
    pub target_id: String,
    /// Presentation label. Empty/blank is treated as a clear (NULL).
    pub display_alias: String,
}

/// Request for `target.display_alias.clear` (gen-3).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetDisplayAliasClearRequest {
    pub target_id: String,
}

// ── Spec 023 US2/US3/US4 DTOs — sessions, projects, notes ────────────────────

/// A linked acquisition session returned by `target.sessions.list` (spec 023 US2).
///
/// Only columns reliably present in `acquisition_session` are surfaced:
/// - `id` — row UUID.
/// - `session_key` — the composite grouping key (pipe-delimited
///   `target|filter|binning|gain|night`, per `sessions::SessionKey::new`) —
///   caller can parse it further if needed.
/// - `created_at` — RFC 3339 UTC timestamp the row was created.
/// - `frame_count` — length of the `frame_ids` JSON array (computed via
///   `json_array_length`; 0 for legacy rows with the default `'[]'`).
/// - `filter` — the filter segment of `session_key` (FR-003/US2-AC1, #739);
///   `""` when the session has no filter (e.g. an unfiltered OSC capture).
///
/// Spec 041 FR-051 (T076): no `state` field — sessions are derived,
/// already-confirmed inventory with no review lifecycle.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetSessionItem {
    pub id: String,
    /// The composite `session_key` grouping key (see struct docs for shape).
    pub session_key: String,
    /// RFC 3339 UTC creation timestamp.
    pub created_at: String,
    /// Number of frames in `frame_ids` JSON array.
    pub frame_count: i64,
    /// Filter segment of `session_key`; `""` when the session has no filter.
    pub filter: String,
}

/// A linked project returned by `target.projects.list` (spec 023 US3).
///
/// Columns sourced from the `projects` table:
/// - `id` — row UUID.
/// - `name` — human-visible project name.
/// - `lifecycle` — lifecycle state string (e.g. `"ready"`, `"processing"`, `"done"`).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetProjectItem {
    pub id: String,
    pub name: String,
    /// Lifecycle state string (e.g. `"ready"`, `"processing"`, `"done"`).
    pub lifecycle: String,
}

/// Request for `target.sessions.list` (spec 023 US2).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetSessionsListRequest {
    pub target_id: String,
}

/// Request for `target.projects.list` (spec 023 US3).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetProjectsListRequest {
    pub target_id: String,
}

/// Request for `target.note.get` (spec 023 US4).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetNoteGetRequest {
    pub target_id: String,
}

/// Request for `target.note.update` (spec 023 US4).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetNoteUpdateRequest {
    pub target_id: String,
    /// New notes text. Empty/whitespace-only clears (stores NULL).
    pub notes: String,
}

/// Response for `target.note.get` (spec 023 US4).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetNoteGetResult {
    /// Current notes, or `null` when none are stored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Response for `target.note.update` (spec 023 US4).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetNoteUpdateResult {
    /// Notes after the update, or `null` when cleared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

// ── Spec 051 US2 DTOs — target favourites ────────────────────────────────────

/// Response for `targets.favourites.list` (spec 051 US2).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetFavouritesListResult {
    /// Ids of every currently-favourited canonical target.
    pub target_ids: Vec<String>,
}

/// Request for `targets.favourites.add` / `targets.favourites.remove` (spec 051 US2).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetFavouriteRequest {
    pub target_id: String,
}

/// Response for `targets.favourites.add` (spec 051 US2).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetFavouriteAddResult {
    pub target_id: String,
    /// ISO-8601 UTC timestamp the target was first favourited.
    pub favourited_at: String,
}

/// Response for `targets.favourites.remove` (spec 051 US2).
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetFavouriteRemoveResult {
    pub target_id: String,
}

// ── Spec 035 DTOs — SIMBAD target resolution ──────────────────────────────────
//
// These types implement the three JSON Schema contracts in
// `specs/035-simbad-target-resolution/contracts/`:
// - `target.search.json`
// - `target.resolve.json`
// - `target.resolution-settings.json`
//
// Pure DTOs (no logic): wire parity with the contracts is verified by T009.

/// Closed astronomical object classification (spec 035 `ObjectType`).
///
/// Mapped uniformly from SIMBAD `otype`; unknown values map to `Other`.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum TargetObjectType {
    Galaxy,
    PlanetaryNebula,
    EmissionNebula,
    ReflectionNebula,
    DarkNebula,
    OpenCluster,
    GlobularCluster,
    SupernovaRemnant,
    GalaxyCluster,
    DoubleStar,
    Asterism,
    Other,
}

/// Closed catalogue identifier slug (spec 035 `CatalogId`).
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum TargetCatalogId {
    Messier,
    Caldwell,
    Sharpless,
    AbellPn,
    AbellGalaxies,
    Arp,
    Vdb,
    Barnard,
    Lbn,
    Ldn,
    Melotte,
    Common,
    Openngc,
}

/// Provenance of a canonical target identity (spec 035).
///
/// `UserOverride` serializes as the hyphenated wire form `user-override`
/// (DTO↔wire parity, T009); the other variants are lower-case words.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, Type,
)]
pub enum TargetSource {
    #[serde(rename = "seed")]
    Seed,
    #[serde(rename = "resolved")]
    Resolved,
    #[serde(rename = "user-override")]
    UserOverride,
}

// ── target.search ─────────────────────────────────────────────────────────────

/// A single ranked typeahead suggestion (`target.search.json` §`Suggestion`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetSuggestion {
    pub target_id: String,
    pub primary_designation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub common_name: Option<String>,
    pub object_type: TargetObjectType,
    /// The alias that matched the query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_alias: Option<String>,
    pub source: TargetSource,
}

/// Request for `target.search` (`target.search.json` §`Request`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetSearchRequest {
    pub contract_version: String,
    pub request_id: String,
    /// Partial designation or common name.
    pub query: String,
    /// Optional; empty/absent = all catalogues.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub catalog_filter: Vec<TargetCatalogId>,
    /// Optional; empty/absent = all types.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_filter: Vec<TargetObjectType>,
    #[serde(default = "default_search_limit")]
    pub limit: u32,
}

fn default_search_limit() -> u32 {
    20
}

/// Response for `target.search` (`target.search.json` §`Response`).
///
/// Local matches only; ordered by match quality. Long-tail/SIMBAD results
/// arrive via `target.resolve`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetSearchResponse {
    pub contract_version: String,
    pub request_id: String,
    pub suggestions: Vec<TargetSuggestion>,
    /// Whether the shared resolve cache is still running its background
    /// seed/durable-row re-warm (startup, or after `target.cache.clear`) —
    /// issue #818: a query that lands mid-warm can get a legitimate-looking
    /// empty result for an object the seed does contain, simply because it
    /// hasn't committed yet. `true` tells the caller a retry may still find
    /// it; `false` means whatever `suggestions` holds is the settled answer.
    /// Always `false` from this pure use case directly (its own unit tests
    /// exercise no `AppState`) — the `target.search` Tauri command sets the
    /// real value from the live warm flag after calling `search`.
    #[serde(default)]
    pub cache_warming: bool,
}

// ── target.resolve ────────────────────────────────────────────────────────────

/// Discriminated status for `target.resolve` (`target.resolve.json` §`ResolveStatus`).
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum TargetResolveStatus {
    /// A canonical target was determined (from cache or SIMBAD).
    Resolved,
    /// Unknown/garbled, or SIMBAD unreachable with no cached entry — marked
    /// pending, retryable; coordinates never fabricated.
    Unresolved,
}

/// Canonical identity returned by `target.resolve` (`target.resolve.json` §`ResolvedTarget`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedTarget {
    pub target_id: String,
    /// SIMBAD physical-object id (dedup key) when resolved online.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub simbad_oid: Option<i64>,
    pub primary_designation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub common_name: Option<String>,
    pub object_type: TargetObjectType,
    /// ICRS J2000 right ascension in `[0, 360)` decimal degrees.
    pub ra_deg: f64,
    /// ICRS J2000 declination in `[-90, 90]` decimal degrees.
    pub dec_deg: f64,
    pub aliases: Vec<String>,
    pub source: TargetSource,
}

/// Closed error codes for `target.resolve` (`target.resolve.json` §`ErrorCode`).
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, Type,
)]
pub enum TargetResolveErrorCode {
    #[serde(rename = "resolver.unreachable")]
    ResolverUnreachable,
    #[serde(rename = "resolver.disabled")]
    ResolverDisabled,
    #[serde(rename = "resolver.timeout")]
    ResolverTimeout,
    #[serde(rename = "actor.not_authorised")]
    ActorNotAuthorised,
}

/// Error envelope for `target.resolve` (`target.resolve.json` §`ErrorEnvelope`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetResolveError {
    pub code: TargetResolveErrorCode,
    pub message: String,
}

/// Manual user-override directive (`target.resolve.json` §`Request.override`).
///
/// When present, binds `query` to this canonical target; persisted as
/// `source=user-override` and wins over future SIMBAD results (FR-014).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetResolveOverride {
    pub target_id: String,
}

/// Request for `target.resolve` (`target.resolve.json` §`Request`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetResolveSimbadRequest {
    pub contract_version: String,
    pub request_id: String,
    /// Complete designation or common name, or a FITS OBJECT value.
    pub query: String,
    /// When present, records a manual user override.
    #[serde(rename = "override", skip_serializing_if = "Option::is_none")]
    pub override_target: Option<TargetResolveOverride>,
}

/// Response for `target.resolve` (`target.resolve.json` §`Response`).
///
/// `target` is present when `status = Resolved`; `unresolvedReason` is present
/// when `status = Unresolved`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetResolveSimbadResponse {
    pub contract_version: String,
    pub request_id: String,
    pub status: TargetResolveStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<ResolvedTarget>,
    /// Present when `status = Unresolved` (e.g. `"unknown"`, `"offline"`,
    /// `"ambiguous"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unresolved_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<TargetResolveError>,
}

// ── target.resolution.settings ────────────────────────────────────────────────

/// SIMBAD resolver settings (`target.resolution-settings.json` §`Settings`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolverSettings {
    /// Enable/disable online SIMBAD resolution (FR-015; default true). When
    /// false, only seed+cache are used.
    pub online_enabled: bool,
    pub simbad_endpoint: String,
    pub debounce_ms: u32,
    pub request_timeout_secs: u32,
}

/// Get request for resolver settings (`target.resolution-settings.json` §`GetRequest`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolverSettingsGetRequest {
    pub contract_version: String,
    pub request_id: String,
    /// Discriminant; always `"get"`.
    pub op: String,
}

/// Update request for resolver settings (`target.resolution-settings.json` §`UpdateRequest`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolverSettingsUpdateRequest {
    pub contract_version: String,
    pub request_id: String,
    /// Discriminant; always `"update"`.
    pub op: String,
    pub settings: ResolverSettings,
}

/// Response for resolver settings get/update (`target.resolution-settings.json` §`Response`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolverSettingsResponse {
    pub contract_version: String,
    pub request_id: String,
    pub settings: ResolverSettings,
}

// ── target.astro_format.batch (adopt target-match) ───────────────────────────

/// One target's RA/Dec to format, for `target.astro_format.batch`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetAstroFormatItem {
    /// Caller-supplied id echoed back on the matching [`TargetAstroFormat`]
    /// (opaque to this command — a `canonical_target.id` in practice).
    pub id: String,
    pub ra_deg: f64,
    pub dec_deg: f64,
}

/// Request for `target.astro_format.batch`: sexagesimal RA/Dec formatting for
/// N targets in a single call, never per-row round trips.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetAstroFormatBatchRequest {
    pub targets: Vec<TargetAstroFormatItem>,
}

/// One target's sexagesimal-formatted RA/Dec.
///
/// Absent from the response when its input RA/Dec was non-finite — never a
/// fabricated string (callers key on `id` to look up a result and fall back
/// to an explicit "unknown" display for ids with no match).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetAstroFormat {
    pub id: String,
    /// `HH:MM:SS` (0 fractional-second digits, carry-safe rounding).
    pub ra_sexagesimal: String,
    /// `±DD:MM:SS` (0 fractional-second digits, carry-safe rounding).
    pub dec_sexagesimal: String,
}

/// Response for `target.astro_format.batch`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetAstroFormatBatchResponse {
    pub formatted: Vec<TargetAstroFormat>,
}

// ── Spec 052 P1: in-use promotion + resolve-cache clear ─────────────────────

/// Request for `target.adopt` — promote a redb-cache-only target (a `target_id`
/// a prior `target.search`/`target.resolve` response returned) into the
/// durable `canonical_target` table. The explicit in-use commit for UI flows
/// with no other natural commit point (e.g. the Targets-page "Add Target"
/// dialog; favouriting/project-create/session-link promote inline as part of
/// their own commands).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetAdoptRequest {
    pub request_id: String,
    pub target_id: String,
}

/// Response for `target.adopt`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetAdoptResponse {
    pub target_id: String,
    /// `false` when `target_id` is unknown to both the redb cache and
    /// `canonical_target` — never fabricated.
    pub adopted: bool,
}

/// Response for `target.cache.clear` (FR-002): the redb resolve cache is
/// wiped and re-warmed from the bundled seed + existing durable
/// `canonical_target` rows. Never touches `canonical_target` itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetCacheClearResponse {
    /// Number of entries the cache was re-warmed with after clearing.
    pub rewarmed_count: u32,
}
