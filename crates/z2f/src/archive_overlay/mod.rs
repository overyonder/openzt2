use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io,
    path::{Path, PathBuf},
    sync::{Mutex, RwLock},
};

use zip::ZipArchive;

use crate::{
    paths::normalize_asset_path_for_case_insensitive_archive_lookup, ArchiveSet,
    EnabledArchiveOverlayState, ResolvedArchiveEntry, ResolvedAssetChange, ResolvedAssetChangeKind,
    WinningArchiveEntryLocation, Z2fArchiveEnabledStatus, REQUIRED_ARCHIVES,
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
        let mut open_zip_archives = Vec::new();
        let mut archive_entries_by_normalized_path = Vec::new();
        for path in paths {
            let name = PathBuf::from(
                path.file_name()
                    .ok_or_else(|| io::Error::other("Z2F path has no filename"))?
                    .to_string_lossy()
                    .to_ascii_lowercase(),
            );
            let mut archive = ZipArchive::new(File::open(&path)?).map_err(io::Error::other)?;
            let archive_priority_index = open_zip_archives.len();
            let mut archive_entries = BTreeMap::new();
            for entry_index in 0..archive.len() {
                let entry = archive.by_index(entry_index).map_err(io::Error::other)?;
                if entry.is_dir() {
                    continue;
                }
                let Some(source) = entry.enclosed_name() else {
                    continue;
                };
                archive_entries.insert(
                    normalize_asset_path_for_case_insensitive_archive_lookup(&source),
                    WinningArchiveEntryLocation {
                        archive_priority_index,
                        zip_entry_index: entry_index,
                        original_archive_path: source,
                    },
                );
            }
            archive_names.push(name);
            open_zip_archives.push(Mutex::new(archive));
            archive_entries_by_normalized_path.push(archive_entries);
        }
        let archive_enabled_states = vec![true; open_zip_archives.len()];
        let (winning_entries_by_normalized_path, resolved_directories) =
            select_winning_entries_and_resolved_directories(
                &archive_entries_by_normalized_path,
                &archive_enabled_states,
            );
        let unique_basename_aliases =
            collect_unique_basename_aliases(&winning_entries_by_normalized_path);
        Ok(Self {
            archive_names,
            open_zip_archives,
            archive_entries_by_normalized_path,
            overlay_state: RwLock::new(EnabledArchiveOverlayState {
                archive_enabled_states,
                winning_entries_by_normalized_path,
                unique_basename_aliases,
                resolved_directories,
                overlay_revision: 0,
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

    #[must_use]
    pub fn overlay_revision(&self) -> u64 {
        self.overlay_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .overlay_revision
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
        let old_winners = std::mem::take(&mut state.winning_entries_by_normalized_path);
        let (winners, directories) = select_winning_entries_and_resolved_directories(
            &self.archive_entries_by_normalized_path,
            &state.archive_enabled_states,
        );
        let old_aliases = std::mem::take(&mut state.unique_basename_aliases);
        let aliases = collect_unique_basename_aliases(&winners);
        let changed_paths = old_winners
            .keys()
            .chain(winners.keys())
            .filter(|path| old_winners.get(*path) != winners.get(*path))
            .chain(
                old_aliases
                    .keys()
                    .chain(aliases.keys())
                    .filter(|path| old_aliases.get(*path) != aliases.get(*path)),
            )
            .cloned()
            .collect::<BTreeSet<_>>();
        let changed = changed_paths
            .into_iter()
            .map(|path| {
                let old = old_winners.get(&path).or_else(|| old_aliases.get(&path));
                let new = winners.get(&path).or_else(|| aliases.get(&path));
                ResolvedAssetChange {
                    normalized_asset_path: path,
                    change_kind: match (old, new) {
                        (None, Some(_)) => ResolvedAssetChangeKind::Added,
                        (Some(_), None) => ResolvedAssetChangeKind::Removed,
                        (Some(_), Some(_)) => ResolvedAssetChangeKind::Modified,
                        (None, None) => unreachable!("changed path must have an old or new winner"),
                    },
                }
            })
            .collect();
        state.winning_entries_by_normalized_path = winners;
        state.unique_basename_aliases = aliases;
        state.resolved_directories = directories;
        state.overlay_revision = state.overlay_revision.wrapping_add(1);
        drop(state);
        Ok(changed)
    }

    /// Returns the current overlay revision and its winning source entries.
    pub fn resolved_winning_entries(&self) -> (u64, Vec<ResolvedArchiveEntry>) {
        let state = self
            .overlay_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let revision = state.overlay_revision;
        let mut entries = state
            .winning_entries_by_normalized_path
            .iter()
            .map(|(path, winner)| ResolvedArchiveEntry {
                normalized_asset_path: path.clone(),
                original_archive_path: winner.original_archive_path.clone(),
                archive_priority_index: winner.archive_priority_index,
                zip_entry_ordinal: winner.zip_entry_index,
            })
            .collect::<Vec<_>>();
        drop(state);
        entries
            .sort_unstable_by_key(|entry| (entry.archive_priority_index, entry.zip_entry_ordinal));
        (revision, entries)
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
        self.archive_entries_by_normalized_path
            .iter()
            .zip(&state.archive_enabled_states)
            .enumerate()
            .filter(|(_, (_, enabled))| **enabled)
            .find_map(|(archive_priority_index, (entries, _))| {
                entries
                    .get(&normalized_path)
                    .map(|entry| (archive_priority_index, entry.zip_entry_index))
            })
    }
}

fn is_required_archive(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| REQUIRED_ARCHIVES.contains(&name))
}

fn select_winning_entries_and_resolved_directories(
    entries: &[BTreeMap<PathBuf, WinningArchiveEntryLocation>],
    enabled: &[bool],
) -> (
    BTreeMap<PathBuf, WinningArchiveEntryLocation>,
    BTreeSet<PathBuf>,
) {
    let winners = entries
        .iter()
        .zip(enabled)
        .filter(|(_, enabled)| **enabled)
        .flat_map(|(entries, _)| entries.iter())
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
