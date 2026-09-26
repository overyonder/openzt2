//! Live, priority-ordered access to assets stored in Z2F archives.
//!
//! This crate owns archive indexing, enabled state, winning-path resolution,
//! and narrow source-text repair. Archive reads preserve original paths and
//! bytes; typed consumers invoke text repair explicitly.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    path::PathBuf,
    sync::{Mutex, RwLock},
};

mod archive_overlay;
mod archive_reading;
pub mod paths;
mod source_reference_resolution;
#[cfg(test)]
mod tests;
pub mod text;

use zip::ZipArchive;

const REQUIRED_ARCHIVES: [&str; 3] = ["x300_000.z2f", "x301_000.z2f", "x302_000.z2f"];

#[derive(Clone, PartialEq, Eq)]
struct WinningArchiveEntryLocation {
    archive_priority_index: usize, // Position in the enabled priority-ordered archive set
    zip_entry_index: usize,        // Entry index used to read the already-open ZIP archive
    original_archive_path: PathBuf, // Original enclosed path used to classify the source format
}

/// An ordered set of enabled Z2F archives exposed as one normalized filesystem.
/// Later enabled archives have higher priority.
pub struct ArchiveSet {
    archive_names: Vec<PathBuf>,
    open_zip_archives: Vec<Mutex<ZipArchive<File>>>,
    archive_entries_by_normalized_path: Vec<BTreeMap<PathBuf, WinningArchiveEntryLocation>>,
    overlay_state: RwLock<EnabledArchiveOverlayState>,
}

struct EnabledArchiveOverlayState {
    archive_enabled_states: Vec<bool>,
    winning_entries_by_normalized_path: BTreeMap<PathBuf, WinningArchiveEntryLocation>,
    unique_basename_aliases: BTreeMap<PathBuf, WinningArchiveEntryLocation>,
    resolved_directories: BTreeSet<PathBuf>,
    overlay_revision: u64,
}

pub struct Z2fArchiveEnabledStatus {
    pub archive_name: PathBuf,
    pub is_enabled: bool,
    pub is_required_base_archive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolvedAssetChangeKind {
    Added,
    Modified,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedAssetChange {
    pub normalized_asset_path: PathBuf,
    pub change_kind: ResolvedAssetChangeKind,
}

/// One winning entry in the current archive overlay.
#[derive(Clone)]
pub struct ResolvedArchiveEntry {
    /// Normalized path used to read the winning entry.
    pub normalized_asset_path: PathBuf,
    /// Original archive path used to classify its source format.
    pub original_archive_path: PathBuf,
    /// Enabled archive position; larger values have higher priority.
    pub archive_priority_index: usize,
    /// Original ZIP entry order within that archive.
    pub zip_entry_ordinal: usize,
}
