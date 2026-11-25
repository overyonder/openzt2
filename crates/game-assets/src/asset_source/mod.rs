//! Bevy's view of the currently enabled Z2F archive overlay.

pub(crate) mod archive_entry_asset_path;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
};

use bevy::{
    asset::io::{
        AssetReader, AssetReaderError, AssetSourceBuilder, AssetSourceEvent, AssetWatcher,
        PathStream, Reader, VecReader,
    },
    prelude::{
        App, DetectChangesMut, First, IntoScheduleConfigs, Message, MessageWriter, Plugin, ResMut,
        Resource, resource_changed,
    },
    tasks::futures_lite::stream,
};

use crate::model_source::gltf_model_source::{
    gltf_model_asset_path_for_source_path, gltf_model_replacement_source_paths,
    gltf_model_source_path_for_asset_path, is_gltf_model_source_path,
};

use crate::world_definitions::{
    ground_path_surface_images::GROUND_PATH_SURFACE_IMAGE_EXTENSION,
    world_definition_asset_loading::authored_binder_reference_resolution::AuthoredBinderSources,
    world_definition_asset_set_refresh_and_indexing::{
        game_context_binder_directory_loading::load_game_context_binder_directories,
        world_definition_source_path,
    },
};

fn load_fossil_puzzle_root(archives: &AssetArchives) -> std::io::Result<PathBuf> {
    use crate::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    };
    use std::io;
    const MANAGER_PATH: &str = "config/puzzlemgr.xml";
    let bytes = archives.read_source(Path::new(MANAGER_PATH))?;
    let config = parse_blue_fang_source_document(AssetPath::new(MANAGER_PATH), &bytes)
        .map_err(io::Error::other)?;
    let root = config
        .root
        .attribute_named_any(&["puzzleRoot"])
        .filter(|root| !root.trim().is_empty())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "ZTPuzzleMgr lacks puzzleRoot")
        })?;
    Ok(z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup(Path::new(root)))
}

pub fn create_bevy_asset_source_for_enabled_z2f_archives(
    enabled_archive_paths: Vec<PathBuf>,
) -> std::io::Result<(AssetSourceBuilder, AssetArchives)> {
    let installation_root = enabled_archive_paths
        .first()
        .and_then(|path| path.parent())
        .map(Path::to_owned)
        .ok_or_else(|| std::io::Error::other("Z2F archive has no installation directory"))?;
    let files = Arc::new(z2f::ArchiveSet::open(enabled_archive_paths)?);
    let events = Arc::new(Mutex::new(None));
    let resolved = Arc::default();
    let shared_source_byte_cache = Arc::new(RwLock::new(SharedSourceByteCache::default()));
    let watcher_events = Arc::clone(&events);
    let archives = AssetArchives {
        files,
        installation_root,
        events,
        resolved,
        shared_source_byte_cache,
        live_model_replacements: Arc::default(),
        pending_overlay_changes: Vec::new(),
    };
    archives.refresh_archive_discovery(None);
    let reader_archives = archives.clone();
    Ok((
        AssetSourceBuilder::new(move || {
            Box::new(Z2fAssetReader {
                archives: reader_archives.clone(),
            })
        })
        .with_watcher(move |sender| {
            *watcher_events
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sender);
            Some(Box::new(Z2fAssetWatcher))
        }),
        archives,
    ))
}

#[derive(Resource, Clone)]
pub struct AssetArchives {
    files: Arc<z2f::ArchiveSet>,
    installation_root: PathBuf,
    events: Arc<Mutex<Option<async_channel::Sender<AssetSourceEvent>>>>,
    resolved: Arc<RwLock<ResolvedArchiveDiscovery>>,
    pending_overlay_changes: Vec<z2f::ResolvedAssetChange>,
    shared_source_byte_cache: Arc<RwLock<SharedSourceByteCache>>,
    /// glTF models sent from an editor while the game runs, by the native
    /// model path they replace. They last until restored or the game exits.
    live_model_replacements: Arc<RwLock<BTreeMap<PathBuf, Arc<[u8]>>>>,
}

const MAXIMUM_SHARED_SOURCE_BYTE_CACHE_BYTES: usize = 64 * 1024 * 1024;
const MAXIMUM_SINGLE_SHARED_SOURCE_BYTE_CACHE_ENTRY_BYTES: usize = 1024 * 1024;

#[derive(Default)]
struct SharedSourceByteCache {
    entries_read_once: BTreeSet<z2f::ArchiveReadEntry>,
    bytes_by_entry: BTreeMap<z2f::ArchiveReadEntry, Arc<[u8]>>,
    retained_bytes: usize,
}

impl SharedSourceByteCache {
    fn retain_reused_entry(&mut self, entry: z2f::ArchiveReadEntry, bytes: &[u8]) {
        if bytes.len() > MAXIMUM_SINGLE_SHARED_SOURCE_BYTE_CACHE_ENTRY_BYTES
            || self.retained_bytes.saturating_add(bytes.len())
                > MAXIMUM_SHARED_SOURCE_BYTE_CACHE_BYTES
            || self.bytes_by_entry.contains_key(&entry)
            || self.entries_read_once.insert(entry)
        {
            return;
        }
        self.retained_bytes += bytes.len();
        self.bytes_by_entry.insert(entry, Arc::from(bytes));
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ArchiveCatalogue {
    Audio,
    Scenarios,
    WorldDefinitionSupplements,
}

impl ArchiveCatalogue {
    const ALL: [Self; 3] = [
        Self::Audio,
        Self::Scenarios,
        Self::WorldDefinitionSupplements,
    ];

    fn contains(self, path: &Path) -> bool {
        match self {
            Self::Audio => crate::audio::audio_asset_index_refresh::is_audio_catalogue_source(path),
            Self::Scenarios => crate::world_scenario::world_scenario_asset_set_refresh_and_indexing::scenario_catalogue_path(path),
            Self::WorldDefinitionSupplements => {
                path.extension().is_some_and(|extension| extension == "xml")
                    && (matches!(path.parent(), Some(parent) if parent == Path::new("xpinfo"))
                        || path.starts_with("biomes/autoplacement")
                        || path == Path::new("config/ztshowtimermanager.xml"))
            }
        }
    }
}

#[derive(Default)]
struct ResolvedArchiveDiscovery {
    ordered_paths: Arc<[PathBuf]>,
    ordered_entries: Arc<[z2f::ResolvedArchiveEntry]>,
    entries_by_path: BTreeMap<PathBuf, z2f::ResolvedArchiveEntry>,
    catalogues: [Arc<[z2f::ResolvedArchiveEntry]>; ArchiveCatalogue::ALL.len()],
    binder_sources: Arc<AuthoredBinderSources>,
    puzzle_root: Option<PathBuf>,
    world_definition_entries: Arc<[z2f::ResolvedArchiveEntry]>,
}

fn archive_entry_order(entry: &z2f::ResolvedArchiveEntry) -> (usize, usize) {
    (entry.archive_priority_index, entry.zip_entry_ordinal)
}

#[derive(Message)]
pub struct ArchiveOverlayChanged {
    pub changes: Vec<z2f::ResolvedAssetChange>,
}

pub(super) struct ArchiveOverlayPublicationPlugin;

impl Plugin for ArchiveOverlayPublicationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ArchiveOverlayChanged>().add_systems(
            First,
            publish_archive_overlay_changes.run_if(resource_changed::<AssetArchives>),
        );
    }
}

fn publish_archive_overlay_changes(
    mut archives: ResMut<AssetArchives>,
    mut changes: MessageWriter<ArchiveOverlayChanged>,
) {
    if !archives.pending_overlay_changes.is_empty() {
        changes.write(ArchiveOverlayChanged {
            changes: std::mem::take(
                &mut archives.bypass_change_detection().pending_overlay_changes,
            ),
        });
    }
}

impl AssetArchives {
    pub fn read_source(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        let path = z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup(path);
        #[cfg(feature = "tracing")]
        let _read = bevy::log::info_span!("read_asset_source", path = %path.display()).entered();
        let archive_entry = self.files.resolve_archive_read_entry(&path);
        let loose = match archive_entry {
            Some(_) if path.components().count() == 1 => self
                .resolved_winning_entry(&path)
                .filter(|entry| entry.normalized_asset_path != path)
                .and_then(|_| self.resolve_loose_path(&path).ok())
                .filter(|path| path.is_file()),
            _ => None,
        };
        let (source, result) = if let Some(loose) = loose {
            ("loose", std::fs::read(loose))
        } else if let Some(entry) = archive_entry {
            self.read_archive_source_entry(entry)
        } else {
            (
                "loose",
                self.resolve_loose_path(&path).and_then(std::fs::read),
            )
        };
        #[cfg(not(feature = "tracing"))]
        let _ = source;
        #[cfg(feature = "tracing")]
        if let Ok(bytes) = &result {
            bevy::log::debug!(target: "openzt2_profile_counts", source, bytes = bytes.len(), "asset read");
        }
        result
    }

    fn read_archive_source_entry(
        &self,
        entry: z2f::ArchiveReadEntry,
    ) -> (&'static str, std::io::Result<Vec<u8>>) {
        let cached = self
            .shared_source_byte_cache
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .bytes_by_entry
            .get(&entry)
            .cloned();
        if let Some(bytes) = cached {
            return ("memory-cache", Ok(bytes.as_ref().to_vec()));
        }
        let result = self.files.read_resolved_archive_entry(entry, usize::MAX);
        if let Ok(bytes) = &result {
            self.shared_source_byte_cache
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .retain_reused_entry(entry, bytes);
        }
        ("archive", result)
    }

    pub(crate) fn source_exists(&self, path: &Path) -> bool {
        self.resolved_winning_entry(path)
            .is_some_and(|entry| entry.normalized_asset_path == path)
            || self
                .resolve_loose_path(path)
                .is_ok_and(|path| path.is_file())
    }

    pub fn resolved_winning_entry(&self, path: &Path) -> Option<z2f::ResolvedArchiveEntry> {
        self.files.resolved_winning_entry(path)
    }

    pub fn resolved_paths(&self) -> Arc<[PathBuf]> {
        Arc::clone(
            &self
                .resolved
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .ordered_paths,
        )
    }

    /// Resolved archive winners in increasing precedence order.
    pub fn resolved_archive_entries(&self) -> Arc<[z2f::ResolvedArchiveEntry]> {
        Arc::clone(
            &self
                .resolved
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .ordered_entries,
        )
    }

    pub(crate) fn catalogue_sources(
        &self,
        catalogue: ArchiveCatalogue,
    ) -> Arc<[z2f::ResolvedArchiveEntry]> {
        Arc::clone(
            &self
                .resolved
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .catalogues[catalogue as usize],
        )
    }

    pub(crate) fn world_definition_sources(
        &self,
    ) -> (Arc<AuthoredBinderSources>, Arc<[z2f::ResolvedArchiveEntry]>) {
        let discovery = self
            .resolved
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (
            Arc::clone(&discovery.binder_sources),
            Arc::clone(&discovery.world_definition_entries),
        )
    }

    pub(crate) fn fossil_puzzle_root(&self) -> Option<PathBuf> {
        self.resolved
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .puzzle_root
            .clone()
    }

    pub(crate) fn authored_binder_sources(&self) -> Arc<AuthoredBinderSources> {
        Arc::clone(
            &self
                .resolved
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .binder_sources,
        )
    }

    fn refresh_archive_discovery(&self, changes: Option<&[z2f::ResolvedAssetChange]>) {
        let mut discovery = self
            .resolved
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(changes) = changes {
            for change in changes {
                let path = &change.normalized_asset_path;
                if let Some(entry) = self
                    .files
                    .resolved_winning_entry(path)
                    .filter(|entry| entry.normalized_asset_path == *path)
                {
                    discovery.entries_by_path.insert(path.clone(), entry);
                } else {
                    discovery.entries_by_path.remove(path);
                }
            }
        } else {
            discovery.entries_by_path = self
                .files
                .resolved_winning_entries()
                .into_iter()
                .map(|entry| (entry.normalized_asset_path.clone(), entry))
                .collect();
        }
        let relevant = |accepts: &dyn Fn(&Path) -> bool| {
            changes.is_none_or(|changes| {
                changes
                    .iter()
                    .any(|change| accepts(&change.normalized_asset_path))
            })
        };
        let mut entries = discovery
            .entries_by_path
            .values()
            .cloned()
            .collect::<Vec<_>>();
        entries.sort_unstable_by_key(archive_entry_order);
        let paths: Arc<[PathBuf]> = entries
            .iter()
            .map(|entry| entry.normalized_asset_path.clone())
            .collect::<Vec<_>>()
            .into();
        if discovery.ordered_paths != paths {
            discovery.ordered_paths = paths;
        }
        for catalogue in ArchiveCatalogue::ALL {
            if relevant(&|path| catalogue.contains(path)) {
                let selected: Arc<[z2f::ResolvedArchiveEntry]> = entries
                    .iter()
                    .filter(|entry| catalogue.contains(&entry.normalized_asset_path))
                    .cloned()
                    .collect::<Vec<_>>()
                    .into();
                let current = &mut discovery.catalogues[catalogue as usize];
                if *current != selected {
                    *current = selected;
                }
            }
        }
        let context_changed = relevant(&|path| path == Path::new("config/gamectxt.xml"));
        let previous_binders = &discovery.binder_sources;
        let binder_sources_changed =
            context_changed || relevant(&|path| previous_binders.contains_path(path));
        if binder_sources_changed {
            let directories = if context_changed {
                load_game_context_binder_directories(self).or_else(|error| {
                    if error.kind() == std::io::ErrorKind::NotFound {
                        Ok(BTreeSet::new())
                    } else {
                        bevy::log::warn!(%error, "could not read game context binder directories");
                        Err(error)
                    }
                })
            } else {
                previous_binders.copy_directories()
            };
            discovery.binder_sources = Arc::new(AuthoredBinderSources::new(directories));
        }
        if discovery.ordered_entries.as_ref() != entries.as_slice() {
            discovery.ordered_entries = Arc::from(entries.clone());
        }
        let puzzle_manager_changed = relevant(&|path| path == Path::new("config/puzzlemgr.xml"));
        if puzzle_manager_changed {
            discovery.puzzle_root = match load_fossil_puzzle_root(self) {
                Ok(root) => Some(root),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => {
                    bevy::log::warn!(%error, "could not read fossil puzzle directory");
                    None
                }
            };
        }
        let is_puzzle = |path: &Path| {
            discovery.puzzle_root.as_deref()
                .is_some_and(|root| crate::world_definitions::world_definition_asset_set_refresh_and_indexing::is_fossil_puzzle_source(path, root))
        };
        if binder_sources_changed
            || puzzle_manager_changed
            || relevant(&|path| {
                world_definition_source_path(path, discovery.binder_sources.directories())
                    || is_puzzle(path)
            })
        {
            discovery.world_definition_entries = entries
                .into_iter()
                .filter(|entry| {
                    world_definition_source_path(
                        &entry.normalized_asset_path,
                        discovery.binder_sources.directories(),
                    ) || is_puzzle(&entry.normalized_asset_path)
                })
                .collect::<Vec<_>>()
                .into();
        }
    }

    pub fn first_enabled_archive_entry_order(&self, path: &Path) -> Option<[u64; 2]> {
        self.files.first_enabled_archive_entry_order(path).and_then(
            |(archive_priority_index, zip_entry_ordinal)| {
                Some([
                    u64::try_from(archive_priority_index).ok()?,
                    u64::try_from(zip_entry_ordinal).ok()?,
                ])
            },
        )
    }

    pub fn enabled_archive_statuses(&self) -> Vec<z2f::Z2fArchiveEnabledStatus> {
        self.files.archive_enabled_statuses()
    }

    pub fn resolve_source_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        self.files
            .resolve_source_reference(from, reference)
            .or_else(|| self.resolve_loose_reference(from, reference))
    }

    pub fn resolve_effect_include(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        self.files
            .resolve_effect_include(from, reference)
            .or_else(|| self.resolve_loose_reference(from, reference))
    }

    pub fn resolve_audio_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        self.files
            .resolve_audio_reference(from, reference)
            .or_else(|| self.resolve_loose_reference(from, reference))
    }

    pub fn resolve_model_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        self.files
            .resolve_model_reference(from, reference)
            .or_else(|| self.resolve_loose_reference(from, reference))
    }

    pub fn resolve_model_reference_with_blue_fang_bfb_preference(
        &self,
        from: &Path,
        reference: &str,
        prefer_blue_fang_bfb: bool,
    ) -> Option<PathBuf> {
        self.files
            .resolve_model_reference_with_blue_fang_bfb_preference(
                from,
                reference,
                prefer_blue_fang_bfb,
            )
            .or_else(|| self.resolve_loose_reference(from, reference))
    }

    pub fn resolve_model_texture_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        self.files
            .resolve_model_texture_reference(from, reference)
            .or_else(|| self.resolve_loose_reference(from, reference))
    }

    pub fn resolve_ui_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        self.files
            .resolve_ui_reference_without_basename_fallback(from, reference)
            .or_else(|| self.resolve_loose_reference(from, reference))
            .or_else(|| self.files.resolve_ui_reference(from, reference))
    }

    /// Lists one explicitly authored source subtree after overlay selection.
    /// This is a bounded dependency lookup, not an overlay enumeration.
    pub fn source_paths_under(&self, directory: &Path) -> Vec<PathBuf> {
        fn visit_archives(files: &z2f::ArchiveSet, directory: &Path, paths: &mut Vec<PathBuf>) {
            if let Ok(children) = files.read_directory(directory) {
                for child in children {
                    if files.is_directory(&child) {
                        visit_archives(files, &child, paths);
                    } else {
                        paths.push(child);
                    }
                }
            }
        }
        fn visit_loose(installation_root: &Path, directory: &Path, paths: &mut Vec<PathBuf>) {
            if let Ok(children) = std::fs::read_dir(directory) {
                for child in children.filter_map(Result::ok) {
                    let Ok(file_type) = child.file_type() else {
                        continue;
                    };
                    let path = child.path();
                    if file_type.is_dir() {
                        visit_loose(installation_root, &path, paths);
                    } else if file_type.is_file() {
                        if let Ok(relative) = path.strip_prefix(installation_root) {
                            paths.push(z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup(relative));
                        }
                    }
                }
            }
        }
        let mut paths = Vec::new();
        visit_archives(&self.files, directory, &mut paths);
        if let Ok(directory) = self.resolve_loose_path(directory) {
            visit_loose(&self.installation_root, &directory, &mut paths);
        }
        paths.sort_unstable();
        paths.dedup();
        paths
    }

    pub fn resolve_script_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        const EXTENSIONS: [&str; 3] = ["lua", "bin", "luac"];
        let normalize = z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup;
        let reference = normalize(Path::new(reference.trim()));
        let explicitly_relative = reference.starts_with(".") || reference.starts_with("..");
        let from = normalize(from);
        let directory = from.parent().unwrap_or_else(|| Path::new(""));
        let exact = |path: PathBuf| {
            let mut canonical = PathBuf::new();
            for component in path.components() {
                match component {
                    Component::Normal(name) => canonical.push(name),
                    Component::CurDir => {}
                    Component::ParentDir if canonical.pop() => {}
                    _ => return None,
                }
            }
            if canonical.extension().is_none_or(|extension| {
                !EXTENSIONS
                    .iter()
                    .any(|allowed| extension.eq_ignore_ascii_case(allowed))
            }) {
                return None;
            }
            self.source_exists(&canonical).then_some(canonical)
        };
        let root = (!explicitly_relative).then(|| reference.clone());
        let local = directory.join(&reference);
        let stem = reference.with_extension("");
        let found = root
            .into_iter()
            .chain(std::iter::once(local))
            .chain(EXTENSIONS.into_iter().flat_map(|extension| {
                let candidate = stem.with_extension(extension);
                (!explicitly_relative)
                    .then(|| candidate.clone())
                    .into_iter()
                    .chain(std::iter::once(directory.join(candidate)))
            }))
            .find_map(exact);
        found.or_else(|| {
            if explicitly_relative {
                return None;
            }
            self.files
                .resolve_script_reference(&from, reference.to_str()?)
                .map(|path| normalize(&path))
        })
    }

    fn resolve_loose_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        let reference = z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup(
            Path::new(reference.trim()),
        );
        let from = z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup(from);
        let parent = from.parent().unwrap_or_else(|| Path::new(""));
        [reference.clone(), parent.join(reference)]
            .into_iter()
            .find(|candidate| {
                self.resolve_loose_path(candidate)
                    .is_ok_and(|path| path.is_file())
            })
    }

    fn resolve_loose_path(&self, path: &Path) -> std::io::Result<PathBuf> {
        let normalized = z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup(path);
        let mut physical = self.installation_root.clone();
        let mut depth = 0_u32;
        for component in normalized.components() {
            let name = match component {
                Component::Normal(name) => name,
                Component::CurDir => continue,
                Component::ParentDir if depth > 0 => {
                    physical.pop();
                    depth -= 1;
                    continue;
                }
                _ => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "asset path must stay within the installation directory",
                    ));
                }
            };
            depth += 1;
            let exact = physical.join(name);
            if exact.exists() {
                physical = exact;
                continue;
            }
            let mut matches = std::fs::read_dir(&physical)?
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .file_name()
                        .as_encoded_bytes()
                        .eq_ignore_ascii_case(name.as_encoded_bytes())
                })
                .map(|entry| entry.path());
            physical = matches.next().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("asset {} was not found", path.display()),
                )
            })?;
            if matches.next().is_some() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("asset {} has ambiguous filename casing", path.display()),
                ));
            }
        }
        Ok(physical)
    }

    /// The glTF model that replaces a native model, if any: a live
    /// replacement first, then `foo.glb` or `foo.gltf` beside `foo.bfb` or
    /// `foo.nif` in the same or a higher-priority archive. Loose files rank
    /// below every archive, as they do for ordinary reads.
    pub fn gltf_model_replacement(
        &self,
        native_model_path: &Path,
    ) -> std::io::Result<Option<Arc<[u8]>>> {
        if let Some(bytes) = self.live_model_replacement(native_model_path) {
            return Ok(Some(bytes));
        }
        let native_model_path =
            z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup(native_model_path);
        let native_archive_priority = self
            .files
            .resolved_winning_entry(&native_model_path)
            .map(|entry| entry.archive_priority_index);
        for replacement_path in gltf_model_replacement_source_paths(&native_model_path) {
            let replaces_native_model = match self.files.resolved_winning_entry(&replacement_path) {
                Some(entry) => native_archive_priority
                    .is_none_or(|priority| entry.archive_priority_index >= priority),
                None => {
                    native_archive_priority.is_none()
                        && self
                            .resolve_loose_path(&replacement_path)
                            .is_ok_and(|path| path.is_file())
                }
            };
            if replaces_native_model {
                return self
                    .read_source(&replacement_path)
                    .map(|bytes| Some(bytes.into()));
            }
        }
        Ok(None)
    }

    /// Reads the source file behind a Bevy asset path, such as `foo.dds` for
    /// `foo.z2dds`.
    pub fn read_asset_source(&self, asset_path: &Path) -> std::io::Result<Vec<u8>> {
        self.read_source(&source_path_for_asset_path(asset_path))
    }

    /// The folder holding the game's Z2F archives.
    pub fn installation_root(&self) -> &Path {
        &self.installation_root
    }

    /// Every model replaced from an editor, with the glTF replacing it.
    pub fn live_model_replacements(&self) -> Vec<(PathBuf, Arc<[u8]>)> {
        self.live_model_replacements
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(|(path, bytes)| (path.clone(), Arc::clone(bytes)))
            .collect()
    }

    /// The glTF sent from an editor to replace a model while the game runs.
    pub fn live_model_replacement(&self, model_path: &Path) -> Option<Arc<[u8]>> {
        self.live_model_replacements
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup(model_path))
            .cloned()
    }

    /// Replaces a model with glTF until it is restored, and reloads it.
    pub fn replace_model_live(
        &self,
        native_model_path: &Path,
        gltf_bytes: Vec<u8>,
    ) -> std::io::Result<()> {
        let native_model_path =
            z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup(native_model_path);
        self.live_model_replacements
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(native_model_path.clone(), gltf_bytes.into());
        self.notify_modified_asset(native_model_path)
    }

    /// Restores one live-replaced model and reloads it.
    /// Returns whether the model had been replaced.
    pub fn restore_live_model_replacement(&self, model_path: &Path) -> std::io::Result<bool> {
        let model_path =
            z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup(model_path);
        let removed = self
            .live_model_replacements
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&model_path);
        if removed.is_some() {
            self.notify_modified_asset(model_path)?;
        }
        Ok(removed.is_some())
    }

    /// Restores every live-replaced model and reloads it, returning the
    /// models restored.
    pub fn restore_live_model_replacements(&self) -> std::io::Result<Vec<PathBuf>> {
        let restored = std::mem::take(
            &mut *self
                .live_model_replacements
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        let restored = restored.into_keys().collect::<Vec<_>>();
        for model_path in &restored {
            self.notify_modified_asset(model_path.clone())?;
        }
        Ok(restored)
    }

    fn notify_modified_asset(&self, path: PathBuf) -> std::io::Result<()> {
        let sender = self
            .events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(sender) = sender {
            sender
                .try_send(AssetSourceEvent::ModifiedAsset(path))
                .map_err(std::io::Error::other)?;
        }
        Ok(())
    }

    /// Changes one archive and returns every physical or compatibility Bevy
    /// asset path notified through the source watcher.
    pub fn set_archive_enabled(
        &mut self,
        archive: usize,
        enabled: bool,
    ) -> std::io::Result<Vec<PathBuf>> {
        let changed = self.files.set_archive_enabled(archive, enabled)?;
        if !changed.is_empty() {
            self.refresh_archive_discovery(Some(&changed));
            self.pending_overlay_changes.extend(changed.iter().cloned());
        }
        let changed_bevy_asset_paths = changed
            .iter()
            .flat_map(|change| {
                asset_paths_reading_source_path(&change.normalized_asset_path)
                    .into_iter()
                    .chain(native_model_asset_paths_replaced_by_source_path(
                        &change.normalized_asset_path,
                    ))
            })
            .collect::<Vec<_>>();
        let sender = self
            .events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(sender) = sender {
            for change in &changed {
                for replaced_native_model_path in
                    native_model_asset_paths_replaced_by_source_path(&change.normalized_asset_path)
                {
                    sender
                        .try_send(AssetSourceEvent::ModifiedAsset(replaced_native_model_path))
                        .map_err(std::io::Error::other)?;
                }
                for changed_bevy_asset_path in
                    asset_paths_reading_source_path(&change.normalized_asset_path)
                {
                    let event = match change.change_kind {
                        z2f::ResolvedAssetChangeKind::Added => {
                            AssetSourceEvent::AddedAsset(changed_bevy_asset_path)
                        }
                        z2f::ResolvedAssetChangeKind::Modified => {
                            AssetSourceEvent::ModifiedAsset(changed_bevy_asset_path)
                        }
                        z2f::ResolvedAssetChangeKind::Removed => {
                            sender
                                .try_send(AssetSourceEvent::RemovedAsset(
                                    changed_bevy_asset_path.clone(),
                                ))
                                .map_err(std::io::Error::other)?;
                            AssetSourceEvent::ModifiedAsset(changed_bevy_asset_path)
                        }
                    };
                    sender.try_send(event).map_err(std::io::Error::other)?;
                }
            }
        }
        Ok(changed_bevy_asset_paths)
    }
}

struct Z2fAssetWatcher;

impl AssetWatcher for Z2fAssetWatcher {}

struct Z2fAssetReader {
    archives: AssetArchives,
}

impl AssetReader for Z2fAssetReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        let result = if let Some((source_path, archive, ordinal)) =
            archive_entry_asset_path::archive_entry_asset_source_path(path)
        {
            self.archives
                .files
                .locate_archive_entry(&source_path, archive, ordinal)
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::NotFound, "no such archive entry")
                })
                .and_then(|entry| self.archives.read_archive_source_entry(entry).1)
        } else {
            self.archives.read_source(&source_path_for_asset_path(path))
        };
        result.map(VecReader::new).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                AssetReaderError::NotFound(path.to_owned())
            } else {
                error.into()
            }
        })
    }

    async fn read_meta<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        Err::<VecReader, _>(AssetReaderError::NotFound(path.to_owned()))
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<PathStream>, AssetReaderError> {
        let mut paths = match self.archives.files.read_directory(path) {
            Ok(paths) => paths,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        if let Ok(children) = self
            .archives
            .resolve_loose_path(path)
            .and_then(std::fs::read_dir)
        {
            paths.extend(children.filter_map(Result::ok).filter_map(|child| {
                child
                    .path()
                    .strip_prefix(&self.archives.installation_root)
                    .ok()
                    .map(z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup)
            }));
        }
        if paths.is_empty() {
            return Err(AssetReaderError::NotFound(path.to_owned()));
        }
        paths.sort_unstable();
        paths.dedup();
        Ok(Box::new(stream::iter(paths)))
    }

    async fn is_directory<'a>(&'a self, path: &'a Path) -> Result<bool, AssetReaderError> {
        Ok(self.archives.files.is_directory(path)
            || self
                .archives
                .resolve_loose_path(path)
                .is_ok_and(|path| path.is_dir()))
    }
}

/// Source file read for a Bevy asset path. Distinct extensions route some
/// sources through OpenZT2's loaders instead of Bevy's built-in ones.
fn source_path_for_asset_path(path: &Path) -> PathBuf {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(GROUND_PATH_SURFACE_IMAGE_EXTENSION) => {
            source_path_for_asset_path(&path.with_extension(""))
        }
        Some("z2cur") => path.with_extension("cur"),
        Some("z2dds") => path.with_extension("dds"),
        _ => gltf_model_source_path_for_asset_path(path),
    }
}

/// Bevy asset paths that read a source file: the file itself and its routed
/// asset path.
fn asset_paths_reading_source_path(path: &Path) -> Vec<PathBuf> {
    let mut paths = vec![path.to_owned()];
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("cur") => paths.push(path.with_extension("z2cur")),
        Some("dds") => paths.push(path.with_extension("z2dds")),
        _ if is_gltf_model_source_path(path) => {
            paths.push(gltf_model_asset_path_for_source_path(path));
        }
        _ => {}
    }
    paths
}

/// Native model asset paths that a glTF model source may replace.
fn native_model_asset_paths_replaced_by_source_path(path: &Path) -> Vec<PathBuf> {
    if is_gltf_model_source_path(path) {
        vec![path.with_extension("bfb"), path.with_extension("nif")]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{
        fs::File,
        io::Write,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };

    use bevy::{
        app::TaskPoolPlugin,
        asset::{
            Asset, AssetApp, AssetLoader, AssetPlugin, AssetServer, Assets, Handle, LoadContext,
            io::{AssetReader, AssetSourceId, Reader},
        },
        prelude::{App, TypePath},
    };
    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::{
        Z2fAssetReader, archive_entry_asset_path, create_bevy_asset_source_for_enabled_z2f_archives,
    };

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    pub(crate) struct TestDirectory(PathBuf);

    impl TestDirectory {
        pub(crate) fn new() -> std::io::Result<Self> {
            let path = std::env::temp_dir().join(format!(
                "openzt2-bevy-assets-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path)?;
            Ok(Self(path))
        }

        pub(crate) fn archive(
            &self,
            name: &str,
            entries: &[(&str, &[u8])],
        ) -> std::io::Result<PathBuf> {
            let path = self.0.join(name);
            let mut writer = ZipWriter::new(File::create(&path)?);
            for (entry, bytes) in entries {
                writer
                    .start_file(*entry, SimpleFileOptions::default())
                    .map_err(std::io::Error::other)?;
                writer.write_all(bytes)?;
            }
            writer.finish().map_err(std::io::Error::other)?;
            Ok(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[derive(Asset, TypePath)]
    struct DependentText(String);

    #[derive(Default, TypePath)]
    struct DependentTextLoader;

    impl AssetLoader for DependentTextLoader {
        type Asset = DependentText;
        type Settings = ();
        type Error = std::io::Error;

        async fn load(
            &self,
            _reader: &mut dyn Reader,
            _settings: &Self::Settings,
            load_context: &mut LoadContext<'_>,
        ) -> Result<Self::Asset, Self::Error> {
            let bytes = load_context
                .read_asset_bytes("helper.dep")
                .await
                .map_err(std::io::Error::other)?;
            String::from_utf8(bytes)
                .map(DependentText)
                .map_err(std::io::Error::other)
        }

        fn extensions(&self) -> &[&str] {
            &["parent"]
        }
    }

    #[derive(Asset, TypePath)]
    struct CompatibleImageSourceBytes(Vec<u8>);

    #[derive(Default, TypePath)]
    struct CompatibleImageSourceBytesLoader;

    impl AssetLoader for CompatibleImageSourceBytesLoader {
        type Asset = CompatibleImageSourceBytes;
        type Settings = ();
        type Error = std::io::Error;

        async fn load(
            &self,
            reader: &mut dyn Reader,
            _settings: &Self::Settings,
            _load_context: &mut LoadContext<'_>,
        ) -> Result<Self::Asset, Self::Error> {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;
            Ok(CompatibleImageSourceBytes(bytes))
        }

        fn extensions(&self) -> &[&str] {
            &["z2dds"]
        }
    }

    fn update_until(
        app: &mut App,
        handle: &Handle<DependentText>,
        expected: &str,
    ) -> std::io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            app.update();
            if app
                .world()
                .resource::<Assets<DependentText>>()
                .get(handle)
                .is_some_and(|asset| asset.0 == expected)
            {
                return Ok(());
            }
            std::thread::yield_now();
        }
        Err(std::io::Error::other(format!(
            "dependent asset did not load {expected:?}"
        )))
    }

    fn update_until_compatible_image_source_contains_expected_bytes(
        app: &mut App,
        handle: &Handle<CompatibleImageSourceBytes>,
        expected: &[u8],
    ) -> std::io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            app.update();
            if app
                .world()
                .resource::<Assets<CompatibleImageSourceBytes>>()
                .get(handle)
                .is_some_and(|asset| asset.0 == expected)
            {
                return Ok(());
            }
            std::thread::yield_now();
        }
        Err(std::io::Error::other(format!(
            "compatible image source did not load {expected:?}"
        )))
    }

    #[test]
    fn ui_references_prefer_exact_loose_paths_before_archive_basename_fallbacks()
    -> std::io::Result<()> {
        let directory = TestDirectory::new()?;
        let base = directory.archive(
            "x300_000.z2f",
            &[("ui/shared/extinct_icon.dds", b"toolbar icon")],
        )?;
        std::fs::create_dir(directory.0.join("xp"))?;
        std::fs::write(directory.0.join("xp/Extinct_icon.dds"), b"expansion logo")?;
        let (_, archives) = create_bevy_asset_source_for_enabled_z2f_archives(vec![base])?;
        let owner = Path::new("xpinfo/cp2.xml");
        let reference = "xp/Extinct_icon.dds";
        let selected = archives
            .resolve_ui_reference(owner, reference)
            .ok_or_else(|| std::io::Error::other("expansion logo did not resolve"))?;
        assert_eq!(selected, Path::new("xp/extinct_icon.dds"));
        assert_eq!(archives.read_source(&selected)?, b"expansion logo");
        assert_eq!(
            archives.resolve_ui_reference(Path::new("xp/menu.xml"), "Extinct_icon.dds"),
            Some(PathBuf::from("xp/extinct_icon.dds")),
        );
        assert_eq!(
            archives.resolve_ui_reference(owner, "extinct_icon.dds"),
            Some(PathBuf::from("ui/shared/extinct_icon.dds"))
        );

        let patch = directory.archive("patch.z2f", &[("xp/Extinct_icon.dds", b"archive logo")])?;
        let base = directory.0.join("x300_000.z2f");
        let (_, patched_archives) =
            create_bevy_asset_source_for_enabled_z2f_archives(vec![base, patch])?;
        let selected = patched_archives
            .resolve_ui_reference(owner, reference)
            .ok_or_else(|| std::io::Error::other("archived expansion logo did not resolve"))?;
        assert_eq!(patched_archives.read_source(&selected)?, b"archive logo");
        let extensionless = patched_archives
            .resolve_ui_reference(owner, "xp/Extinct_icon")
            .ok_or_else(|| std::io::Error::other("extensionless archived logo did not resolve"))?;
        assert_eq!(
            patched_archives.read_source(&extensionless)?,
            b"archive logo"
        );
        Ok(())
    }

    #[test]
    fn archive_toggle_reloads_assets_that_read_changed_documents() -> std::io::Result<()> {
        let directory = TestDirectory::new()?;
        let base = directory.archive(
            "x300_000.z2f",
            &[("document.parent", b"document"), ("helper.dep", b"base")],
        )?;
        let modification = directory.archive("mod.z2f", &[("helper.dep", b"mod")])?;
        let (source, mut archives) =
            create_bevy_asset_source_for_enabled_z2f_archives(vec![base, modification])?;
        let mut app = App::new();
        app.register_asset_source(AssetSourceId::Default, source)
            .add_plugins((
                TaskPoolPlugin::default(),
                AssetPlugin {
                    watch_for_changes_override: Some(true),
                    use_asset_processor_override: Some(false),
                    ..Default::default()
                },
            ))
            .init_asset::<DependentText>()
            .init_asset_loader::<DependentTextLoader>();
        let handle = app
            .world()
            .resource::<AssetServer>()
            .load::<DependentText>(Path::new("document.parent"));

        update_until(&mut app, &handle, "mod")?;
        archives.set_archive_enabled(1, false)?;
        update_until(&mut app, &handle, "base")?;
        archives.set_archive_enabled(1, true)?;
        update_until(&mut app, &handle, "mod")
    }

    #[test]
    fn archive_toggle_reloads_compatible_image_handle_when_physical_dds_winner_changes()
    -> std::io::Result<()> {
        let directory = TestDirectory::new()?;
        let base = directory.archive("x300_000.z2f", &[("image.dds", b"base")])?;
        let modification = directory.archive("mod.z2f", &[("image.dds", b"mod")])?;
        let (source, mut archives) =
            create_bevy_asset_source_for_enabled_z2f_archives(vec![base, modification])?;
        let mut app = App::new();
        app.register_asset_source(AssetSourceId::Default, source)
            .add_plugins((
                TaskPoolPlugin::default(),
                AssetPlugin {
                    watch_for_changes_override: Some(true),
                    use_asset_processor_override: Some(false),
                    ..Default::default()
                },
            ))
            .init_asset::<CompatibleImageSourceBytes>()
            .init_asset_loader::<CompatibleImageSourceBytesLoader>();
        let handle = app
            .world()
            .resource::<AssetServer>()
            .load::<CompatibleImageSourceBytes>(Path::new("image.z2dds"));

        update_until_compatible_image_source_contains_expected_bytes(&mut app, &handle, b"mod")?;
        let disabled_paths = archives.set_archive_enabled(1, false)?;
        assert_eq!(
            disabled_paths,
            [PathBuf::from("image.dds"), PathBuf::from("image.z2dds")]
        );
        update_until_compatible_image_source_contains_expected_bytes(&mut app, &handle, b"base")?;
        let restored_paths = archives.set_archive_enabled(1, true)?;
        assert_eq!(
            restored_paths,
            [PathBuf::from("image.dds"), PathBuf::from("image.z2dds")]
        );
        update_until_compatible_image_source_contains_expected_bytes(&mut app, &handle, b"mod")
    }

    #[test]
    fn a_late_read_of_an_archive_entry_cannot_replace_the_active_bytes() -> std::io::Result<()> {
        let directory = TestDirectory::new()?;
        let base = directory.archive("x300_000.z2f", &[("config/settings.xml", b"base")])?;
        let modification = directory.archive("mod.z2f", &[("Config/Settings.xml", b"mod")])?;
        let (_, mut archives) =
            create_bevy_asset_source_for_enabled_z2f_archives(vec![base, modification])?;
        let path = Path::new("config/settings.xml");
        let selected = archives
            .resolved_winning_entry(path)
            .ok_or_else(|| std::io::Error::other("missing settings override"))?;
        let old_path = archive_entry_asset_path::archive_entry_asset_path(&selected);
        let reader = Z2fAssetReader {
            archives: archives.clone(),
        };

        archives.set_archive_enabled(1, false)?;
        assert_eq!(archives.read_source(path)?, b"base");
        bevy::tasks::block_on(async {
            for _ in 0..2 {
                let mut input = reader
                    .read(&old_path)
                    .await
                    .map_err(std::io::Error::other)?;
                let mut bytes = Vec::new();
                input.read_to_end(&mut bytes).await?;
                assert_eq!(bytes, b"mod");
            }
            Ok::<(), std::io::Error>(())
        })?;
        assert_eq!(
            archives.read_source(Path::new("CONFIG/SETTINGS.XML"))?,
            b"base"
        );
        Ok(())
    }

    #[test]
    fn effect_include_winner_changes_independently_of_its_parent() -> std::io::Result<()> {
        let directory = TestDirectory::new()?;
        let base = directory.archive(
            "x300_000.z2f",
            &[
                ("effects/novel.fx", b"#include \"shared.fxh\""),
                ("effects/shared.fxh", b"base"),
            ],
        )?;
        let modification =
            directory.archive("mod.z2f", &[("effects/shared.fxh", b"replacement")])?;
        let (_, mut archives) =
            create_bevy_asset_source_for_enabled_z2f_archives(vec![base, modification])?;
        let parent = Path::new("effects/novel.fx");
        let include = archives
            .resolve_effect_include(parent, "shared.fxh")
            .ok_or_else(|| std::io::Error::other("include did not resolve"))?;
        assert_eq!(archives.read_source(&include)?, b"replacement");
        archives.set_archive_enabled(1, false)?;
        assert_eq!(archives.read_source(&include)?, b"base");
        assert_eq!(archives.read_source(parent)?, b"#include \"shared.fxh\"");
        Ok(())
    }
}
