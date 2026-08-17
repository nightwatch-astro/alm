// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

//! Minimal FITS block builder for test fixtures (bd astro-plan-kyo7.88).
//!
//! Replaces ~17 hand-rolled `write_fits*` helper functions scattered across
//! 7 crates. Each was duplicating the FITS block layout: 80-byte cards, END
//! card, 2880-byte block padding. This crate is the single implementation.
//!
//! ## Scope
//!
//! Test-only, consumed as `dev-dependency`. Not for production FITS writing;
//! `fits-header` on crates.io is the right upstream for real header I/O once
//! it gains a write surface.
//!
//! ## Design
//!
//! The builder appends cards to a first 2880-byte block, writes an END card,
//! and pads the block with spaces. For test fixtures one block is always
//! sufficient. The `write(path)` method is the single I/O entry point.
//!
//! ## Why internal, not external crate
//!
//! Test fixtures are workspace implementation details. Publishing to crates.io
//! adds version management overhead for a ~50-line builder used only here.
//! A future real FITS write surface belongs in `fits-header` (crates.io), not
//! this file.

use std::io::Write as _;
use std::path::Path;

/// Minimal FITS fixture builder.
///
/// # Panics
///
/// Panics if more than 35 cards are added (overflows one 2880-byte block
/// after the mandatory END card). Test fixtures never need more than ~10 cards.
#[derive(Default)]
pub struct FitsFixture {
    cards: Vec<String>,
}

impl FitsFixture {
    /// Create an empty fixture builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a raw card string (truncated / padded to 80 bytes).
    #[must_use]
    pub fn card(mut self, raw: &str) -> Self {
        self.cards.push(format!("{:<80}", &raw[..raw.len().min(80)]));
        self
    }

    /// Add an `IMAGETYP` card.
    #[must_use]
    pub fn imagetyp(self, value: &str) -> Self {
        self.card(&format!("IMAGETYP= '{value:<8}'"))
    }

    /// Add an `OBJECT` card.
    #[must_use]
    pub fn object(self, value: &str) -> Self {
        self.card(&format!("OBJECT  = '{value}'"))
    }

    /// Add a `FILTER` card.
    #[must_use]
    pub fn filter(self, value: &str) -> Self {
        self.card(&format!("FILTER  = '{value}'"))
    }

    /// Add a `DATE-OBS` card.
    #[must_use]
    pub fn date_obs(self, value: &str) -> Self {
        self.card(&format!("DATE-OBS= '{value}'"))
    }

    /// Add an `EXPTIME` card.
    #[must_use]
    pub fn exptime(self, value: f64) -> Self {
        self.card(&format!("EXPTIME = {value}"))
    }

    /// Add a `GAIN` card.
    #[must_use]
    pub fn gain(self, value: f64) -> Self {
        self.card(&format!("GAIN    = {value}"))
    }

    /// Add an `INSTRUME` card.
    #[must_use]
    pub fn instrume(self, value: &str) -> Self {
        self.card(&format!("INSTRUME= '{value}'"))
    }

    /// Add a `STACKCNT` card.
    #[must_use]
    pub fn stackcnt(self, value: u32) -> Self {
        self.card(&format!("STACKCNT= {value}"))
    }

    /// Add a `TELESCOP` card.
    #[must_use]
    pub fn telescop(self, value: &str) -> Self {
        self.card(&format!("TELESCOP= '{value}'"))
    }

    /// Add an `RA` card (decimal degrees).
    #[must_use]
    pub fn ra(self, value: f64) -> Self {
        self.card(&format!("RA      = {value}"))
    }

    /// Add a `DEC` card (decimal degrees).
    #[must_use]
    pub fn dec(self, value: f64) -> Self {
        self.card(&format!("DEC     = {value}"))
    }

    /// Write the FITS fixture to `path` (creates or truncates).
    ///
    /// # Panics
    ///
    /// Panics if the file cannot be created or written, or if more than 35
    /// cards were added (would overflow one 2880-byte FITS block).
    pub fn write(self, path: &Path) {
        const BLOCK: usize = 2880;
        const CARD: usize = 80;
        const MAX_CARDS: usize = BLOCK / CARD; // 36

        let n = self.cards.len();
        // END card consumes one slot; guard against overflow.
        assert!(
            n < MAX_CARDS,
            "fits_fixtures: {n} cards + END exceeds one 2880-byte FITS block ({MAX_CARDS} slots)"
        );

        let mut block = vec![b' '; BLOCK];
        for (i, card) in self.cards.iter().enumerate() {
            let bytes = card.as_bytes();
            block[i * CARD..i * CARD + bytes.len().min(CARD)]
                .copy_from_slice(&bytes[..bytes.len().min(CARD)]);
        }
        // END card
        block[n * CARD..n * CARD + 3].copy_from_slice(b"END");

        let mut f = std::fs::File::create(path)
            .unwrap_or_else(|e| panic!("fits_fixtures: cannot create {}: {e}", path.display()));
        f.write_all(&block)
            .unwrap_or_else(|e| panic!("fits_fixtures: write failed for {}: {e}", path.display()));
    }

    /// Convenience: write to `dir / name`.
    pub fn write_to(self, dir: &Path, name: &str) {
        self.write(&dir.join(name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_imagetyp_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.fits");
        FitsFixture::new().imagetyp("Light Frame").write(&path);

        let bytes = std::fs::read(&path).unwrap();
        // Block is exactly 2880 bytes.
        assert_eq!(bytes.len(), 2880);
        // IMAGETYP card starts at offset 0.
        let card = std::str::from_utf8(&bytes[0..80]).unwrap();
        assert!(card.starts_with("IMAGETYP"), "card: {card:?}");
        // END card is at offset 80 (second card position).
        assert!(bytes[80..83] == *b"END", "END not at expected position");
    }

    #[test]
    fn multi_card_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.fits");
        FitsFixture::new().imagetyp("Light Frame").object("M31").filter("L").write(&path);

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), 2880);
        // END should be at slot 3.
        assert_eq!(bytes[3 * 80..3 * 80 + 3], *b"END");
    }
}
