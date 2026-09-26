use std::{
    collections::BTreeSet,
    io::{self, Read},
    path::{Path, PathBuf},
};

use crate::{
    archive_overlay::winning_archive_entry_for_path,
    paths::normalize_asset_path_for_case_insensitive_archive_lookup, ArchiveSet,
};

impl ArchiveSet {
    pub fn contains(&self, path: &Path) -> bool {
        let path = normalize_asset_path_for_case_insensitive_archive_lookup(path);
        let state = self
            .overlay_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        winning_archive_entry_for_path(&state, &path).is_some()
    }

    pub fn is_directory(&self, path: &Path) -> bool {
        let path = normalize_asset_path_for_case_insensitive_archive_lookup(path);
        self.overlay_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .resolved_directories
            .contains(&path)
    }

    /// Reads the unmodified winning entry for a normalized asset path.
    ///
    /// # Errors
    ///
    /// Returns an error when the path has no enabled winner or its archive
    /// entry cannot be read.
    pub fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let path = normalize_asset_path_for_case_insensitive_archive_lookup(path);
        let state = self
            .overlay_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let winner = winning_archive_entry_for_path(&state, &path)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.display().to_string()))?;
        drop(state);
        let mut archive = self.open_zip_archives[winner.archive_priority_index]
            .lock()
            .map_err(|_| io::Error::other("Z2F archive lock poisoned"))?;
        let mut entry = archive
            .by_index(winner.zip_entry_index)
            .map_err(io::Error::other)?;
        let capacity = usize::try_from(entry.size())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Z2F entry is too large"))?;
        let mut bytes = Vec::with_capacity(capacity);
        entry.read_to_end(&mut bytes)?;
        drop(entry);
        drop(archive);
        Ok(bytes)
    }

    /// Lists the immediate children of a directory in the resolved view.
    ///
    /// # Errors
    ///
    /// Returns an error when `path` is not a directory in the resolved view.
    pub fn read_directory(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let path = normalize_asset_path_for_case_insensitive_archive_lookup(path);
        if !self.is_directory(&path) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                path.display().to_string(),
            ));
        }
        let depth = path.components().count() + 1;
        let state = self
            .overlay_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(state
            .winning_entries_by_normalized_path
            .keys()
            .chain(&state.resolved_directories)
            .filter(|candidate| {
                candidate.starts_with(&path) && candidate.components().count() == depth
            })
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }
}
