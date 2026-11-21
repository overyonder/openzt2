//! Read-only member access for selected original saved zoos.

use std::{
    collections::BTreeMap,
    io,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock,
    },
};

use bevy::{
    asset::io::{AssetReader, AssetReaderError, AssetSourceBuilder, PathStream, Reader, VecReader},
    prelude::Resource,
    tasks::futures_lite::stream,
};
use openzt2_game_data::AssetId;

const WORLD_MEMBER: &str = "saved/world.zt2";
const TERRAIN_MEMBER: &str = "saved/terrain.dat";

#[derive(Resource, Clone)]
pub struct OriginalSavedGameSources {
    packages: Arc<RwLock<BTreeMap<String, Arc<z2f::ArchiveSet>>>>,
    next_load: Arc<AtomicU64>,
    maximum_member_byte_count: usize,
}

pub struct OpenOriginalSavedGamePackage {
    sources: OriginalSavedGameSources,
    key: String,
}

impl OpenOriginalSavedGamePackage {
    pub fn world_asset_path(&self) -> String {
        format!("original-saves://{}/{WORLD_MEMBER}", self.key)
    }
}

impl Drop for OpenOriginalSavedGamePackage {
    fn drop(&mut self) {
        self.sources
            .packages
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.key);
    }
}

impl OriginalSavedGameSources {
    pub fn new(maximum_member_byte_count: usize) -> Self {
        Self {
            packages: Default::default(),
            next_load: Default::default(),
            maximum_member_byte_count,
        }
    }

    pub fn asset_source(&self) -> AssetSourceBuilder {
        let sources = self.clone();
        AssetSourceBuilder::new(move || {
            Box::new(OriginalSavedGameReader {
                sources: sources.clone(),
            })
        })
    }

    pub fn source_identifier(path: &Path) -> io::Result<AssetId> {
        let path = path.canonicalize()?;
        Ok(original_saved_game_path_identifier(&path))
    }

    pub fn read_catalogue_world(
        &self,
        path: &Path,
        maximum_bytes: usize,
    ) -> io::Result<(AssetId, Vec<u8>)> {
        let path = path.canonicalize()?;
        let identifier = original_saved_game_path_identifier(&path);
        let archive = open_saved_game_archive(path)?;
        let bytes = archive.read_with_maximum_byte_count(
            Path::new(WORLD_MEMBER),
            maximum_bytes.min(self.maximum_member_byte_count),
        )?;
        Ok((identifier, bytes))
    }

    pub fn open_selected_package(
        &self,
        path: &Path,
        expected: AssetId,
    ) -> io::Result<OpenOriginalSavedGamePackage> {
        let path = path.canonicalize()?;
        if original_saved_game_path_identifier(&path) != expected {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "saved zoo path changed",
            ));
        }
        let archive = open_saved_game_archive(path)?;
        let load = self.next_load.fetch_add(1, Ordering::Relaxed);
        let key = format!(
            "{}-load-{load:x}",
            expected.to_lowercase_hexadecimal_string()
        );
        self.packages
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key.clone(), Arc::new(archive));
        Ok(OpenOriginalSavedGamePackage {
            sources: self.clone(),
            key,
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
        let archive = self
            .packages
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(key)
            .cloned()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "saved zoo package is closed")
            })?;
        Ok((archive, member))
    }
}

pub(crate) fn stable_original_saved_game_member_path(path: &str) -> Option<String> {
    let (package, member) = path.strip_prefix("original-saves://")?.split_once('/')?;
    let (identifier, _) = package.split_once("-load-")?;
    Some(format!("original-saves://{identifier}/{member}"))
}

fn original_saved_game_path_identifier(path: &Path) -> AssetId {
    let mut hash = blake3::Hasher::new();
    hash.update(b"original-saved-game\0");
    hash.update(path.as_os_str().as_encoded_bytes());
    let mut identifier = [0; 16];
    identifier.copy_from_slice(&hash.finalize().as_bytes()[..16]);
    AssetId(identifier)
}

fn open_saved_game_archive(path: PathBuf) -> io::Result<z2f::ArchiveSet> {
    let archive = z2f::ArchiveSet::open([path])?;
    if !archive.contains(Path::new(WORLD_MEMBER)) || !archive.contains(Path::new(TERRAIN_MEMBER)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "saved zoo has no world or terrain member",
        ));
    }
    Ok(archive)
}

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
            return Ok(Box::new(stream::iter(Vec::<PathBuf>::new())));
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
