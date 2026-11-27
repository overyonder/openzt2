use std::{
    collections::BTreeSet,
    io::{self, Read},
    path::{Path, PathBuf},
};

use crate::{
    archive_overlay::winning_archive_entry_for_path,
    paths::normalize_asset_path_for_case_insensitive_archive_lookup, ArchiveReadEntry, ArchiveSet,
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
        self.read_with_maximum_byte_count(path, usize::MAX)
    }

    /// Reads the winning entry with a limit on both its declared and actual size.
    ///
    /// # Errors
    ///
    /// Returns an error if the entry exceeds the limit or cannot be read.
    pub fn read_with_maximum_byte_count(
        &self,
        path: &Path,
        maximum_byte_count: usize,
    ) -> io::Result<Vec<u8>> {
        let winner = self
            .resolve_archive_read_entry(path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.display().to_string()))?;
        self.read_resolved_archive_entry(winner, maximum_byte_count)
    }

    pub fn resolve_archive_read_entry(&self, path: &Path) -> Option<ArchiveReadEntry> {
        let normalized = normalize_asset_path_for_case_insensitive_archive_lookup(path);
        let state = self
            .overlay_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let winner = winning_archive_entry_for_path(&state, &normalized)?;
        Some(ArchiveReadEntry {
            archive_priority_index: winner.archive_priority_index,
            zip_entry_ordinal: winner.zip_entry_index,
            content_revision: winner.content_revision,
        })
    }

    pub fn locate_archive_entry(
        &self,
        path: &Path,
        archive_priority_index: usize,
        zip_entry_ordinal: usize,
    ) -> Option<ArchiveReadEntry> {
        let path = normalize_asset_path_for_case_insensitive_archive_lookup(path);
        let state = self
            .overlay_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let entry = state
            .archives
            .get(archive_priority_index)?
            .entries
            .get(&path)?;
        (entry.zip_entry_index == zip_entry_ordinal).then_some(ArchiveReadEntry {
            archive_priority_index,
            zip_entry_ordinal,
            content_revision: entry.content_revision,
        })
    }

    /// Reads the immutable entry previously selected by this archive set.
    ///
    /// # Errors
    ///
    /// Returns an error when the entry cannot be read or exceeds the byte limit.
    pub fn read_resolved_archive_entry(
        &self,
        winner: ArchiveReadEntry,
        maximum_byte_count: usize,
    ) -> io::Result<Vec<u8>> {
        let state = self
            .overlay_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut archive = state
            .archives
            .get(winner.archive_priority_index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown Z2F archive"))?
            .zip
            .clone();
        drop(state);
        let mut entry = archive
            .by_index(winner.zip_entry_ordinal)
            .map_err(io::Error::other)?;
        if entry.size() != winner.content_revision.uncompressed_size
            || entry.crc32() != winner.content_revision.crc32
        {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Z2F entry was replaced",
            ));
        }
        let capacity = usize::try_from(entry.size())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Z2F entry is too large"))?;
        if capacity > maximum_byte_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Z2F entry exceeds the requested byte limit",
            ));
        }
        let mut bytes = Vec::with_capacity(capacity);
        let read_limit = u64::try_from(maximum_byte_count)
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        (&mut entry).take(read_limit).read_to_end(&mut bytes)?;
        if bytes.len() > maximum_byte_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Z2F entry exceeds the requested byte limit",
            ));
        }
        drop(entry);
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
