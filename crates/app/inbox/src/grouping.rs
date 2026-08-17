// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Pure grouping engine for single-type inbox sub-items (spec 041 T064).
//!
//! Implements the deterministic **group-key recipe** from research R-9 and the
//! rotation/pointing/temperature/time semantics from R-18. Given a file's
//! effective metadata plus a per-frame-type [`GroupingConfig`], it produces a
//! canonical [`GroupKey`] (stable across rescans — FR-042) and a human
//! [`GroupLabel`].
//!
//! This module is a **pure function over metadata** — no DB, no I/O. The
//! persistence/classify integration (materializing sub-items) is a separate
//! task (T066) and does NOT live here.
//!
//! # Recipe summary (R-9, FR-035…FR-040)
//!
//! | type  | default identity dimensions (beyond `frame_type`)                                                                            |
//! |-------|------------------------------------------------------------------------------------------------------------------------------|
//! | light | optic-train(`TELESCOP`+`INSTRUME`+`FOCALLEN`), filter, exposure*, gain, offset, binning, pointing(RA/Dec)†, rotation†, night |
//! | dark  | camera(`INSTRUME`), exposure*, gain, offset, set-temp‡, binning, readout∘                                                     |
//! | bias  | camera, gain, offset, binning, readout∘, night                                                                               |
//! | flat  | camera, optic-train, filter (required), gain, offset, binning, rotation†, readout∘, night                                    |
//!
//! `*` exposure bucketed to canonical seconds. `‡` set-temp bucketed to the
//! configured tolerance. `†` pointing & rotation grouped *within* a tolerance.
//! `∘` readout-mode optional, OFF by default. Lights deliberately do **not**
//! group by temperature (R-9). Rotation uses `ROTATANG` (mechanical), never
//! `OBJCTROT` (R-18).
//!
//! # Module layout
//!
//! Split by cohesion: `metadata` (input view), `dimension` (identity
//! dimensions), `config` (per-type recipe + sentinel), `result` (output
//! types), `engine` (the pure `group_file` computation + its unit tests).
#![allow(clippy::doc_markdown)] // spec/FITS terminology not appropriate for backticks

mod config;
mod dimension;
mod engine;
mod metadata;
mod result;

pub use config::{GroupingConfig, TempSource, SENTINEL_MISSING};
pub use dimension::Dimension;
pub use engine::group_file;
pub use metadata::FrameMetadata;
pub use result::{GroupKey, GroupLabel, GroupResult, GroupWarning};
