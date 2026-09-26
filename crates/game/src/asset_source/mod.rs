//! Bevy's view of the currently enabled Z2F archive overlay.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock,
    },
    time::{Duration, Instant},
};

use bevy::{
    asset::io::{
        AssetReader, AssetReaderError, AssetSourceBuilder, AssetSourceEvent, AssetWatcher,
        PathStream, Reader, VecReader,
    },
    prelude::Resource,
    tasks::futures_lite::stream,
};

pub(crate) fn create_bevy_asset_source_for_enabled_z2f_archives(
    enabled_archive_paths: Vec<PathBuf>,
) -> std::io::Result<(AssetSourceBuilder, AssetArchives)> {
    let installation_root = enabled_archive_paths
        .first()
        .and_then(|path| path.parent())
        .map(Path::to_owned)
        .ok_or_else(|| std::io::Error::other("Z2F archive has no installation directory"))?;
    let files = Arc::new(z2f::ArchiveSet::open(enabled_archive_paths)?);
    let events = Arc::new(Mutex::new(None));
    let resolved = Arc::new(Mutex::new(ResolvedPaths::default()));
    let scene_loading_reads = Arc::new(Mutex::new(
        SceneLoadingAssetReadPerformanceAttribution::default(),
    ));
    let scene_loading_reads_active = Arc::new(AtomicBool::new(false));
    let shared_source_byte_cache = Arc::new(RwLock::new(SharedSourceByteCache::default()));
    let watcher_events = Arc::clone(&events);
    let archives = AssetArchives {
        files,
        installation_root,
        events,
        resolved,
        scene_loading_reads,
        scene_loading_reads_active,
        shared_source_byte_cache,
    };
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
pub(crate) struct AssetArchives {
    files: Arc<z2f::ArchiveSet>,
    installation_root: PathBuf,
    events: Arc<Mutex<Option<async_channel::Sender<AssetSourceEvent>>>>,
    resolved: Arc<Mutex<ResolvedPaths>>,
    scene_loading_reads: Arc<Mutex<SceneLoadingAssetReadPerformanceAttribution>>,
    scene_loading_reads_active: Arc<AtomicBool>,
    shared_source_byte_cache: Arc<RwLock<SharedSourceByteCache>>,
}

const MAXIMUM_SHARED_SOURCE_BYTE_CACHE_BYTES: usize = 64 * 1024 * 1024;
const MAXIMUM_SINGLE_SHARED_SOURCE_BYTE_CACHE_ENTRY_BYTES: usize = 1024 * 1024;

#[derive(Default)]
struct SharedSourceByteCache {
    paths_read_once: BTreeSet<PathBuf>,
    bytes_by_path: BTreeMap<PathBuf, Arc<[u8]>>,
    retained_bytes: usize,
}

impl SharedSourceByteCache {
    fn get(&self, path: &Path) -> Option<Vec<u8>> {
        self.bytes_by_path
            .get(path)
            .map(|bytes| bytes.as_ref().to_vec())
    }

    fn retain_after_read(&mut self, path: &Path, bytes: &[u8]) {
        if self.bytes_by_path.contains_key(path)
            || self.paths_read_once.insert(path.to_owned())
            || bytes.len() > MAXIMUM_SINGLE_SHARED_SOURCE_BYTE_CACHE_ENTRY_BYTES
            || self.retained_bytes.saturating_add(bytes.len())
                > MAXIMUM_SHARED_SOURCE_BYTE_CACHE_BYTES
        {
            return;
        }
        self.retained_bytes = self.retained_bytes.saturating_add(bytes.len());
        self.bytes_by_path.insert(path.to_owned(), Arc::from(bytes));
    }

    fn invalidate(&mut self, path: &Path) {
        self.paths_read_once.remove(path);
        if let Some(bytes) = self.bytes_by_path.remove(path) {
            self.retained_bytes = self.retained_bytes.saturating_sub(bytes.len());
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AssetReadPerformanceAttribution {
    pub(crate) reads: u64,
    pub(crate) bytes: u64,
    pub(crate) elapsed: Duration,
    pub(crate) maximum: Duration,
}

#[derive(Debug, Clone)]
pub(crate) struct SlowAssetReadPerformanceAttribution {
    pub(crate) path: PathBuf,
    pub(crate) source: &'static str,
    pub(crate) bytes: usize,
    pub(crate) elapsed: Duration,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SceneLoadingAssetReadPerformanceSnapshot {
    pub(crate) archive: AssetReadPerformanceAttribution,
    pub(crate) loose: AssetReadPerformanceAttribution,
    pub(crate) memory_cache: AssetReadPerformanceAttribution,
    pub(crate) extensions: BTreeMap<String, AssetReadPerformanceAttribution>,
    pub(crate) paths: BTreeMap<PathBuf, AssetReadPerformanceAttribution>,
    pub(crate) loaders: BTreeMap<&'static str, AssetReadPerformanceAttribution>,
    pub(crate) slowest: Vec<SlowAssetReadPerformanceAttribution>,
}

#[derive(Default)]
struct SceneLoadingAssetReadPerformanceAttribution {
    snapshot: SceneLoadingAssetReadPerformanceSnapshot,
}

impl SceneLoadingAssetReadPerformanceAttribution {
    fn record(&mut self, path: &Path, source: &'static str, bytes: usize, elapsed: Duration) {
        let update = |attribution: &mut AssetReadPerformanceAttribution| {
            attribution.reads = attribution.reads.saturating_add(1);
            attribution.bytes = attribution
                .bytes
                .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX));
            attribution.elapsed = attribution.elapsed.saturating_add(elapsed);
            attribution.maximum = attribution.maximum.max(elapsed);
        };
        update(match source {
            "archive" => &mut self.snapshot.archive,
            "memory-cache" => &mut self.snapshot.memory_cache,
            _ => &mut self.snapshot.loose,
        });
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("<none>")
            .to_ascii_lowercase();
        update(self.snapshot.extensions.entry(extension).or_default());
        update(self.snapshot.paths.entry(path.to_owned()).or_default());
        let read = SlowAssetReadPerformanceAttribution {
            path: path.to_owned(),
            source,
            bytes,
            elapsed,
        };
        let insertion = self
            .snapshot
            .slowest
            .partition_point(|existing| existing.elapsed >= elapsed);
        self.snapshot.slowest.insert(insertion, read);
        self.snapshot.slowest.truncate(20);
    }
}

#[derive(Default)]
struct ResolvedPaths {
    revision: Option<u64>,
    paths: Arc<[PathBuf]>,
}

impl AssetArchives {
    pub(crate) fn measure_scene_loading_asset_translation(
        &self,
        loader: &'static str,
    ) -> SceneLoadingAssetTranslationPerformanceTimer {
        SceneLoadingAssetTranslationPerformanceTimer {
            attribution: self
                .scene_loading_reads_active
                .load(Ordering::Relaxed)
                .then(|| Arc::clone(&self.scene_loading_reads)),
            loader,
            started: Instant::now(),
        }
    }

    pub(crate) fn begin_scene_loading_asset_read_performance_attribution(&self) {
        let mut attribution = self
            .scene_loading_reads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *attribution = SceneLoadingAssetReadPerformanceAttribution::default();
        self.scene_loading_reads_active
            .store(true, Ordering::Release);
    }

    pub(crate) fn finish_scene_loading_asset_read_performance_attribution(
        &self,
    ) -> SceneLoadingAssetReadPerformanceSnapshot {
        self.scene_loading_reads_active
            .store(false, Ordering::Release);
        let attribution = self
            .scene_loading_reads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        attribution.snapshot.clone()
    }

    pub(crate) fn read_source(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        let path = z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup(path);
        let started = Instant::now();
        let cached = self
            .shared_source_byte_cache
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&path);
        let (source, result) = if let Some(bytes) = cached {
            ("memory-cache", Ok(bytes))
        } else if self.files.contains(&path) {
            ("archive", self.files.read(&path))
        } else {
            (
                "loose",
                self.resolve_loose_path(&path).and_then(std::fs::read),
            )
        };
        if source != "memory-cache" {
            if let Ok(bytes) = &result {
                self.shared_source_byte_cache
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .retain_after_read(&path, bytes);
            }
        }
        if self.scene_loading_reads_active.load(Ordering::Relaxed) {
            if let Ok(bytes) = &result {
                self.scene_loading_reads
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .record(&path, source, bytes.len(), started.elapsed());
            }
        }
        result
    }

    pub(crate) fn revision(&self) -> u64 {
        self.files.overlay_revision()
    }

    pub(crate) fn resolved_paths(&self) -> (u64, Arc<[PathBuf]>) {
        let revision = self.files.overlay_revision();
        {
            let cached = self
                .resolved
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if cached.revision == Some(revision) {
                return (revision, Arc::clone(&cached.paths));
            }
        }
        let (revision, entries) = self.files.resolved_winning_entries();
        let paths = entries
            .into_iter()
            .map(|entry| entry.normalized_asset_path)
            .collect::<Vec<_>>();
        let paths = Arc::<[PathBuf]>::from(paths);
        let mut cached = self
            .resolved
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cached.revision = Some(revision);
        cached.paths = Arc::clone(&paths);
        (revision, paths)
    }

    pub(crate) fn first_enabled_archive_entry_order(&self, path: &Path) -> Option<[u64; 2]> {
        self.files.first_enabled_archive_entry_order(path).and_then(
            |(archive_priority_index, zip_entry_ordinal)| {
                Some([
                    u64::try_from(archive_priority_index).ok()?,
                    u64::try_from(zip_entry_ordinal).ok()?,
                ])
            },
        )
    }

    pub(crate) fn enabled_archive_statuses(&self) -> Vec<z2f::Z2fArchiveEnabledStatus> {
        self.files.archive_enabled_statuses()
    }

    pub(crate) fn resolve_source_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        self.files
            .resolve_source_reference(from, reference)
            .or_else(|| self.resolve_loose_reference(from, reference))
    }

    pub(crate) fn resolve_effect_include(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        self.files
            .resolve_effect_include(from, reference)
            .or_else(|| self.resolve_loose_reference(from, reference))
    }

    pub(crate) fn resolve_audio_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        self.files
            .resolve_audio_reference(from, reference)
            .or_else(|| self.resolve_loose_reference(from, reference))
    }

    pub(crate) fn resolve_model_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        self.files
            .resolve_model_reference(from, reference)
            .or_else(|| self.resolve_loose_reference(from, reference))
    }

    pub(crate) fn resolve_model_reference_with_blue_fang_bfb_preference(
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

    pub(crate) fn resolve_model_texture_reference(
        &self,
        from: &Path,
        reference: &str,
    ) -> Option<PathBuf> {
        self.files
            .resolve_model_texture_reference(from, reference)
            .or_else(|| self.resolve_loose_reference(from, reference))
    }

    pub(crate) fn resolve_ui_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        self.files
            .resolve_ui_reference(from, reference)
            .or_else(|| self.resolve_loose_reference(from, reference))
    }

    /// Lists one explicitly authored source subtree after overlay selection.
    /// This is a bounded dependency lookup, not an overlay enumeration.
    pub(crate) fn source_paths_under(&self, directory: &Path) -> Vec<PathBuf> {
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

    pub(crate) fn resolve_script_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        self.files
            .resolve_script_reference(from, reference)
            .or_else(|| self.resolve_loose_reference(from, reference))
    }

    fn resolve_loose_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        let reference = z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup(
            Path::new(reference.trim()),
        );
        let parent = z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup(from)
            .parent()?
            .to_owned();
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
                    ))
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

    /// Changes one archive and returns every physical or compatibility Bevy
    /// asset path notified through the source watcher.
    pub(crate) fn set_archive_enabled(
        &self,
        archive: usize,
        enabled: bool,
    ) -> std::io::Result<Vec<PathBuf>> {
        let changed = self.files.set_archive_enabled(archive, enabled)?;
        {
            let mut cache = self
                .shared_source_byte_cache
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for change in &changed {
                cache.invalidate(&change.normalized_asset_path);
            }
        }
        let changed_bevy_asset_paths = changed
            .iter()
            .flat_map(|change| {
                std::iter::once(change.normalized_asset_path.clone()).chain(
                    compatible_bevy_image_asset_path_for_changed_source_path(
                        &change.normalized_asset_path,
                    ),
                )
            })
            .collect::<Vec<_>>();
        let sender = self
            .events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(sender) = sender {
            for change in &changed {
                for changed_bevy_asset_path in std::iter::once(change.normalized_asset_path.clone())
                    .chain(compatible_bevy_image_asset_path_for_changed_source_path(
                        &change.normalized_asset_path,
                    ))
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

pub(crate) struct SceneLoadingAssetTranslationPerformanceTimer {
    attribution: Option<Arc<Mutex<SceneLoadingAssetReadPerformanceAttribution>>>,
    loader: &'static str,
    started: Instant,
}

impl Drop for SceneLoadingAssetTranslationPerformanceTimer {
    fn drop(&mut self) {
        let Some(attribution) = &self.attribution else {
            return;
        };
        let elapsed = self.started.elapsed();
        let mut attribution = attribution
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let loader = attribution.snapshot.loaders.entry(self.loader).or_default();
        loader.reads = loader.reads.saturating_add(1);
        loader.elapsed = loader.elapsed.saturating_add(elapsed);
        loader.maximum = loader.maximum.max(elapsed);
    }
}

struct Z2fAssetWatcher;

impl AssetWatcher for Z2fAssetWatcher {}

struct Z2fAssetReader {
    archives: AssetArchives,
}

impl AssetReader for Z2fAssetReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        let source_path = compatible_image_source_path(path);
        self.archives
            .read_source(&source_path)
            .map(VecReader::new)
            .map_err(|error| {
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

fn compatible_image_source_path(path: &Path) -> PathBuf {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("z2cur") => path.with_extension("cur"),
        Some("z2dds") => path.with_extension("dds"),
        _ => path.to_owned(),
    }
}

fn compatible_bevy_image_asset_path_for_changed_source_path(path: &Path) -> Option<PathBuf> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("cur") => Some(path.with_extension("z2cur")),
        Some("dds") => Some(path.with_extension("z2dds")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
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
            io::{AssetSourceId, Reader},
            Asset, AssetApp, AssetLoader, AssetPlugin, AssetServer, Assets, Handle, LoadContext,
        },
        prelude::{App, TypePath},
    };
    use zip::{write::SimpleFileOptions, ZipWriter};

    use super::create_bevy_asset_source_for_enabled_z2f_archives;

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> std::io::Result<Self> {
            let path = std::env::temp_dir().join(format!(
                "openzt2-bevy-assets-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path)?;
            Ok(Self(path))
        }

        fn archive(&self, name: &str, entries: &[(&str, &[u8])]) -> std::io::Result<PathBuf> {
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
    fn archive_toggle_reloads_assets_that_read_changed_documents() -> std::io::Result<()> {
        let directory = TestDirectory::new()?;
        let base = directory.archive(
            "x300_000.z2f",
            &[("document.parent", b"document"), ("helper.dep", b"base")],
        )?;
        let modification = directory.archive("mod.z2f", &[("helper.dep", b"mod")])?;
        let (source, archives) =
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
    fn archive_toggle_reloads_compatible_image_handle_when_physical_dds_winner_changes(
    ) -> std::io::Result<()> {
        let directory = TestDirectory::new()?;
        let base = directory.archive("x300_000.z2f", &[("image.dds", b"base")])?;
        let modification = directory.archive("mod.z2f", &[("image.dds", b"mod")])?;
        let (source, archives) =
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
        let (_, archives) =
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
