use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io,
    path::{Path, PathBuf},
    sync::RwLock,
};

use zip::ZipArchive;

use crate::{
    paths::normalize_asset_path_for_case_insensitive_archive_lookup,
    positional_archive_reader::PositionalArchiveReader, ArchiveEntryContentRevision, ArchiveSet,
    EnabledArchiveOverlayState, IndexedArchive, ResolvedArchiveEntry, ResolvedAssetChange,
    ResolvedAssetChangeKind, WinningArchiveEntryLocation, Z2fArchiveEnabledStatus,
    REQUIRED_ARCHIVES,
};

impl ArchiveSet {
    /// Opens every archive once and resolves the initial priority view.
    ///
    /// # Errors
    ///
    /// Returns an error when a path has no filename or an archive cannot be
    /// opened or indexed.
    pub fn open(paths: impl IntoIterator<Item = PathBuf>) -> io::Result<Self> {
        let mut archive_names = Vec::new();
        let mut archives = Vec::new();
        for path in paths {
            let name = PathBuf::from(
                path.file_name()
                    .ok_or_else(|| io::Error::other("Z2F path has no filename"))?
                    .to_string_lossy()
                    .to_ascii_lowercase(),
            );
            archives.push(index_archive(&path, archives.len())?);
            archive_names.push(name);
        }
        let archive_enabled_states = vec![true; archives.len()];
        let (winning_entries_by_normalized_path, resolved_directories) =
            select_winning_entries_and_resolved_directories(&archives, &archive_enabled_states);
        let unique_basename_aliases =
            collect_unique_basename_aliases(&winning_entries_by_normalized_path);
        Ok(Self {
            archive_names,
            overlay_state: RwLock::new(EnabledArchiveOverlayState {
                archives,
                archive_enabled_states,
                winning_entries_by_normalized_path,
                unique_basename_aliases,
                resolved_directories,
            }),
        })
    }

    pub fn archive_enabled_statuses(&self) -> Vec<Z2fArchiveEnabledStatus> {
        let state = self
            .overlay_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.archive_names
            .iter()
            .cloned()
            .zip(state.archive_enabled_states.iter().copied())
            .map(|(archive_name, is_enabled)| Z2fArchiveEnabledStatus {
                is_required_base_archive: is_required_archive(&archive_name),
                archive_name,
                is_enabled,
            })
            .collect()
    }

    pub fn archive_source_path(&self, archive_priority_index: usize) -> Option<PathBuf> {
        let state = self
            .overlay_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Some(
            state
                .archives
                .get(archive_priority_index)?
                .source_path
                .clone(),
        )
    }

    /// Changes one archive's enabled state and returns paths whose winner changed.
    ///
    /// # Errors
    ///
    /// Returns an error when `archive` is not in this set or an attempt is made
    /// to disable a required base archive.
    pub fn set_archive_enabled(
        &self,
        archive_priority_index: usize,
        is_enabled: bool,
    ) -> io::Result<Vec<ResolvedAssetChange>> {
        let name = self
            .archive_names
            .get(archive_priority_index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown Z2F archive"))?;
        if !is_enabled && is_required_archive(name) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{} is a required Z2F archive", name.display()),
            ));
        }
        let mut state = self
            .overlay_state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let current = state
            .archive_enabled_states
            .get_mut(archive_priority_index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "incomplete Z2F state"))?;
        if *current == is_enabled {
            return Ok(Vec::new());
        }
        *current = is_enabled;
        let introduction_order_changes = state.archives[archive_priority_index]
            .entries
            .keys()
            .filter(|path| !earlier_archive_contains(&state, archive_priority_index, path))
            .cloned()
            .collect();
        Ok(rebuild_overlay(&mut state, introduction_order_changes))
    }

    /// Reopens the contents of an archive at the same priority position.
    ///
    /// # Errors
    ///
    /// Returns an error when the archive position is unknown or the new ZIP
    /// cannot be opened or indexed. In either case the overlay stays intact.
    pub fn replace_archive(
        &self,
        archive_priority_index: usize,
        path: &Path,
    ) -> io::Result<Vec<ResolvedAssetChange>> {
        if archive_priority_index >= self.archive_names.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unknown Z2F archive",
            ));
        }
        let archive = index_archive(path, archive_priority_index)?;
        let mut state = self
            .overlay_state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = std::mem::replace(&mut state.archives[archive_priority_index], archive);
        if !state.archive_enabled_states[archive_priority_index] {
            return Ok(Vec::new());
        }
        let replacement = &state.archives[archive_priority_index].entries;
        let introduction_order_changes = previous
            .entries
            .keys()
            .chain(replacement.keys())
            .filter(|path| {
                let previous_ordinal = previous
                    .entries
                    .get(*path)
                    .map(|entry| entry.zip_entry_index);
                let replacement_ordinal = replacement.get(*path).map(|entry| entry.zip_entry_index);
                previous_ordinal != replacement_ordinal
                    && !earlier_archive_contains(&state, archive_priority_index, path)
            })
            .cloned()
            .collect();
        Ok(rebuild_overlay(&mut state, introduction_order_changes))
    }

    /// Returns the winning source entries in archive and ZIP order.
    pub fn resolved_winning_entries(&self) -> Vec<ResolvedArchiveEntry> {
        let state = self
            .overlay_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut entries = state
            .winning_entries_by_normalized_path
            .iter()
            .map(|(path, winner)| ResolvedArchiveEntry {
                normalized_asset_path: path.clone(),
                original_archive_path: winner.original_archive_path.clone(),
                archive_priority_index: winner.archive_priority_index,
                zip_entry_ordinal: winner.zip_entry_index,
                content_revision: winner.content_revision,
            })
            .collect::<Vec<_>>();
        drop(state);
        entries
            .sort_unstable_by_key(|entry| (entry.archive_priority_index, entry.zip_entry_ordinal));
        entries
    }

    /// Returns metadata for the same winning entry used by source reads.
    #[must_use]
    pub fn resolved_winning_entry(&self, path: &Path) -> Option<ResolvedArchiveEntry> {
        let path = normalize_asset_path_for_case_insensitive_archive_lookup(path);
        let state = self
            .overlay_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let winner = winning_archive_entry_for_path(&state, &path)?.clone();
        drop(state);
        Some(ResolvedArchiveEntry {
            normalized_asset_path: normalize_asset_path_for_case_insensitive_archive_lookup(
                &winner.original_archive_path,
            ),
            original_archive_path: winner.original_archive_path,
            archive_priority_index: winner.archive_priority_index,
            zip_entry_ordinal: winner.zip_entry_index,
            content_revision: winner.content_revision,
        })
    }

    /// Returns the first enabled archive and ZIP entry which declared a path.
    ///
    /// The original `BFType` registry retained a type's first registration even
    /// when a later archive replaced that path's definition. Catalogue rows
    /// with equal explicit sort keys consequently retain this introduction
    /// order.
    #[must_use]
    pub fn first_enabled_archive_entry_order(&self, path: &Path) -> Option<(usize, usize)> {
        let normalized_path = normalize_asset_path_for_case_insensitive_archive_lookup(path);
        let state = self
            .overlay_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state
            .archives
            .iter()
            .zip(&state.archive_enabled_states)
            .enumerate()
            .filter(|(_, (_, enabled))| **enabled)
            .find_map(|(archive_priority_index, (archive, _))| {
                archive
                    .entries
                    .get(&normalized_path)
                    .map(|entry| (archive_priority_index, entry.zip_entry_index))
            })
    }
}

fn index_archive(path: &Path, archive_priority_index: usize) -> io::Result<IndexedArchive> {
    let file = File::open(path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("Cannot open {}: {error}", path.display()),
        )
    })?;
    let mut zip = ZipArchive::new(PositionalArchiveReader::new(file)?)
        .map_err(|error| io::Error::other(format!("Cannot read {}: {error}", path.display())))?;
    let mut entries = BTreeMap::new();
    for zip_entry_index in 0..zip.len() {
        let entry = zip.by_index(zip_entry_index).map_err(io::Error::other)?;
        if entry.is_dir() {
            continue;
        }
        let Some(source) = entry.enclosed_name() else {
            continue;
        };
        entries.insert(
            normalize_asset_path_for_case_insensitive_archive_lookup(&source),
            WinningArchiveEntryLocation {
                archive_priority_index,
                zip_entry_index,
                original_archive_path: source,
                content_revision: ArchiveEntryContentRevision {
                    uncompressed_size: entry.size(),
                    crc32: entry.crc32(),
                },
            },
        );
    }
    Ok(IndexedArchive {
        source_path: path.to_owned(),
        zip,
        entries,
    })
}

fn earlier_archive_contains(
    state: &EnabledArchiveOverlayState,
    archive_priority_index: usize,
    path: &Path,
) -> bool {
    state.archives[..archive_priority_index]
        .iter()
        .zip(&state.archive_enabled_states)
        .any(|(archive, enabled)| *enabled && archive.entries.contains_key(path))
}

fn rebuild_overlay(
    state: &mut EnabledArchiveOverlayState,
    mut changed_paths: BTreeSet<PathBuf>,
) -> Vec<ResolvedAssetChange> {
    let (winners, directories) = select_winning_entries_and_resolved_directories(
        &state.archives,
        &state.archive_enabled_states,
    );
    let aliases = collect_unique_basename_aliases(&winners);
    let old_winners = std::mem::replace(&mut state.winning_entries_by_normalized_path, winners);
    let old_aliases = std::mem::replace(&mut state.unique_basename_aliases, aliases);
    state.resolved_directories = directories;
    for (old, new) in [
        (&old_winners, &state.winning_entries_by_normalized_path),
        (&old_aliases, &state.unique_basename_aliases),
    ] {
        changed_paths.extend(
            old.keys()
                .chain(new.keys())
                .filter(|path| old.get(*path) != new.get(*path))
                .cloned(),
        );
    }
    changed_paths
        .into_iter()
        .filter_map(|path| {
            let old = old_winners.get(&path).or_else(|| old_aliases.get(&path));
            let new = winning_archive_entry_for_path(state, &path);
            let change_kind = match (old, new) {
                (None, Some(_)) => ResolvedAssetChangeKind::Added,
                (Some(_), None) => ResolvedAssetChangeKind::Removed,
                (Some(_), Some(_)) => ResolvedAssetChangeKind::Modified,
                (None, None) => return None,
            };
            Some(ResolvedAssetChange {
                normalized_asset_path: path,
                change_kind,
            })
        })
        .collect()
}

fn is_required_archive(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| REQUIRED_ARCHIVES.contains(&name))
}

fn select_winning_entries_and_resolved_directories(
    archives: &[IndexedArchive],
    enabled: &[bool],
) -> (
    BTreeMap<PathBuf, WinningArchiveEntryLocation>,
    BTreeSet<PathBuf>,
) {
    let winners = archives
        .iter()
        .zip(enabled)
        .filter(|(_, enabled)| **enabled)
        .flat_map(|(archive, _)| archive.entries.iter())
        .map(|(path, winner)| (path.clone(), winner.clone()))
        .collect::<BTreeMap<_, _>>();
    let directories = winners
        .keys()
        .flat_map(|path| path.ancestors().skip(1).map(Path::to_owned))
        .chain(std::iter::once(PathBuf::new()))
        .collect();
    (winners, directories)
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "the archive lookup is shared by sibling modules, not part of the public archive API"
)]
pub(super) fn winning_archive_entry_for_path<'a>(
    state: &'a EnabledArchiveOverlayState,
    path: &Path,
) -> Option<&'a WinningArchiveEntryLocation> {
    state
        .winning_entries_by_normalized_path
        .get(path)
        .or_else(|| {
            (path.components().count() == 1)
                .then(|| state.unique_basename_aliases.get(path))
                .flatten()
        })
}

fn collect_unique_basename_aliases(
    winners: &BTreeMap<PathBuf, WinningArchiveEntryLocation>,
) -> BTreeMap<PathBuf, WinningArchiveEntryLocation> {
    let mut aliases = BTreeMap::<PathBuf, Option<WinningArchiveEntryLocation>>::new();
    for (path, winner) in winners {
        let Some(name) = path.file_name().map(PathBuf::from) else {
            continue;
        };
        aliases
            .entry(name)
            .and_modify(|alias| *alias = None)
            .or_insert_with(|| Some(winner.clone()));
    }
    aliases
        .into_iter()
        .filter_map(|(path, winner)| winner.map(|winner| (path, winner)))
        .collect()
}
