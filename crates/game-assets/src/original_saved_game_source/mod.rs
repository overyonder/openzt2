//! Read-only member access for saved zoos outside the installation archives.

use std::{
    collections::BTreeMap,
    io,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    time::SystemTime,
};

use bevy::{
    asset::io::{
        AssetReader, AssetReaderError, AssetSourceBuilder, AssetSourceEvent, AssetWatcher,
        PathStream, Reader, VecReader,
    },
    prelude::Resource,
    tasks::futures_lite::stream,
};
use openzt2_game_data::AssetId;

#[derive(Resource, Clone)]
pub struct OriginalSavedGameSources {
    archives: Arc<RwLock<BTreeMap<String, OriginalSavedGameArchive>>>,
    events: Arc<Mutex<Option<async_channel::Sender<AssetSourceEvent>>>>,
    maximum_member_byte_count: usize,
}

struct OriginalSavedGameArchive {
    path: PathBuf,
    length: u64,
    modified: Option<SystemTime>,
    files: Arc<z2f::ArchiveSet>,
}

impl OriginalSavedGameSources {
    pub fn new(maximum_member_byte_count: usize) -> Self {
        Self {
            archives: Default::default(),
            events: Default::default(),
            maximum_member_byte_count,
        }
    }

    pub fn asset_source(&self) -> AssetSourceBuilder {
        let sources = self.clone();
        let events = Arc::clone(&self.events);
        AssetSourceBuilder::new(move || {
            Box::new(OriginalSavedGameReader {
                sources: sources.clone(),
            })
        })
        .with_watcher(move |sender| {
            *events
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sender);
            Some(Box::new(OriginalSavedGameWatcher))
        })
    }

    /// Registers the original file without extracting or changing any member.
    pub fn register(&self, path: &Path) -> io::Result<AssetId> {
        let path = path.canonicalize()?;
        let metadata = path.metadata()?;
        if !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "saved zoo is not a file",
            ));
        }
        let mut hash = blake3::Hasher::new();
        hash.update(b"original-saved-game\0");
        hash.update(path.as_os_str().as_encoded_bytes());
        let mut identifier = [0; 16];
        identifier.copy_from_slice(&hash.finalize().as_bytes()[..16]);
        let identifier = AssetId(identifier);
        let key = identifier.to_lowercase_hexadecimal_string();
        let length = metadata.len();
        let modified = metadata.modified().ok();
        {
            let archives = self
                .archives
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if archives.get(&key).is_some_and(|archive| {
                archive.path == path && archive.length == length && archive.modified == modified
            }) {
                return Ok(identifier);
            }
        }
        let files = Arc::new(z2f::ArchiveSet::open([path.clone()])?);
        if !files.contains(Path::new("saved/world.zt2"))
            || !files.contains(Path::new("saved/terrain.dat"))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "saved zoo has no world or terrain member",
            ));
        }
        let replaced = self
            .archives
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                key.clone(),
                OriginalSavedGameArchive {
                    path,
                    length,
                    modified,
                    files,
                },
            );
        if replaced.is_some() {
            if let Some(sender) = self
                .events
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
            {
                for member in [
                    "saved/world.zt2",
                    "saved/terrain.dat",
                    "saved/thumbnail.jpg",
                ] {
                    sender
                        .try_send(AssetSourceEvent::ModifiedAsset(
                            PathBuf::from(&key).join(member),
                        ))
                        .map_err(io::Error::other)?;
                }
            }
        }
        Ok(identifier)
    }

    pub fn member_path(identifier: AssetId, member: &Path) -> PathBuf {
        PathBuf::from(identifier.to_lowercase_hexadecimal_string()).join(member)
    }

    pub fn read_member(
        &self,
        identifier: AssetId,
        member: &Path,
        maximum_byte_count: usize,
    ) -> io::Result<Vec<u8>> {
        let files = self.archive(&identifier.to_lowercase_hexadecimal_string())?;
        files.read_with_maximum_byte_count(
            member,
            maximum_byte_count.min(self.maximum_member_byte_count),
        )
    }

    fn archive(&self, key: &str) -> io::Result<Arc<z2f::ArchiveSet>> {
        self.archives
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(key)
            .map(|archive| Arc::clone(&archive.files))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "saved zoo source is not registered",
                )
            })
    }

    fn resolve(&self, path: &Path) -> io::Result<(Arc<z2f::ArchiveSet>, PathBuf)> {
        let mut components = path.components();
        let Some(Component::Normal(key)) = components.next() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "saved zoo member has no source",
            ));
        };
        let key = key.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "saved zoo source is not text")
        })?;
        let mut member = PathBuf::new();
        for component in components {
            let Component::Normal(name) = component else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid saved zoo member path",
                ));
            };
            member.push(name);
        }
        Ok((self.archive(key)?, member))
    }
}

struct OriginalSavedGameWatcher;
impl AssetWatcher for OriginalSavedGameWatcher {}

struct OriginalSavedGameReader {
    sources: OriginalSavedGameSources,
}

impl AssetReader for OriginalSavedGameReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        self.sources
            .resolve(path)
            .and_then(|(files, member)| {
                files.read_with_maximum_byte_count(&member, self.sources.maximum_member_byte_count)
            })
            .map(VecReader::new)
            .map_err(|error| asset_reader_error(path, error))
    }

    async fn read_meta<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        Err::<VecReader, _>(AssetReaderError::NotFound(path.to_owned()))
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<PathStream>, AssetReaderError> {
        if path.as_os_str().is_empty() {
            let paths = self
                .sources
                .archives
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .keys()
                .map(PathBuf::from)
                .collect::<Vec<_>>();
            return Ok(Box::new(stream::iter(paths)));
        }
        let (files, member) = self
            .sources
            .resolve(path)
            .map_err(|error| asset_reader_error(path, error))?;
        let prefix = path
            .components()
            .next()
            .expect("resolved source prefix")
            .as_os_str();
        let paths = files
            .read_directory(&member)
            .map_err(|error| asset_reader_error(path, error))?
            .into_iter()
            .map(|member| PathBuf::from(prefix).join(member))
            .collect::<Vec<_>>();
        Ok(Box::new(stream::iter(paths)))
    }

    async fn is_directory<'a>(&'a self, path: &'a Path) -> Result<bool, AssetReaderError> {
        if path.as_os_str().is_empty() {
            return Ok(true);
        }
        let (files, member) = self
            .sources
            .resolve(path)
            .map_err(|error| asset_reader_error(path, error))?;
        Ok(member.as_os_str().is_empty() || files.is_directory(&member))
    }
}

fn asset_reader_error(path: &Path, error: io::Error) -> AssetReaderError {
    if error.kind() == io::ErrorKind::NotFound {
        AssetReaderError::NotFound(path.to_owned())
    } else {
        error.into()
    }
}
