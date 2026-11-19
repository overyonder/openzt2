use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use crate::{
    asset_source::AssetArchives,
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
        source_document_semantic_name::canonicalize_source_document_record_key,
    },
};

#[derive(Debug)]
pub(crate) struct AuthoredBinderSources {
    directories: io::Result<BTreeSet<PathBuf>>,
    declarations: OnceLock<BTreeMap<String, Vec<PathBuf>>>,
}

impl Default for AuthoredBinderSources {
    fn default() -> Self {
        Self::new(Ok(BTreeSet::new()))
    }
}

impl AuthoredBinderSources {
    pub(crate) fn new(directories: io::Result<BTreeSet<PathBuf>>) -> Self {
        Self {
            directories,
            declarations: OnceLock::new(),
        }
    }

    pub(crate) fn directories(&self) -> &BTreeSet<PathBuf> {
        static EMPTY: BTreeSet<PathBuf> = BTreeSet::new();
        self.directories.as_ref().unwrap_or(&EMPTY)
    }

    pub(crate) fn copy_directories(&self) -> io::Result<BTreeSet<PathBuf>> {
        self.directories
            .as_ref()
            .cloned()
            .map_err(|error| io::Error::new(error.kind(), error.to_string()))
    }

    pub(crate) fn contains_path(&self, path: &Path) -> bool {
        path.parent()
            .is_some_and(|parent| self.directories().contains(parent))
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
    }

    pub(super) fn resolve_binder_type_source_path(
        &self,
        archives: &AssetArchives,
        from: &str,
        binder_type: &str,
    ) -> io::Result<Option<PathBuf>> {
        let directories = self
            .directories
            .as_ref()
            .map_err(|error| io::Error::new(error.kind(), error.to_string()))?;
        let declarations = self.declarations.get_or_init(|| {
            let mut paths_by_type = BTreeMap::<String, Vec<PathBuf>>::new();
            for path in directories.iter().flat_map(|directory| archives.source_paths_under(directory))
                .filter(|path| self.contains_path(path)).collect::<BTreeSet<_>>()
            {
                let document = archives.read_source(&path).and_then(|bytes| {
                    parse_blue_fang_source_document(AssetPath::new(&path.to_string_lossy()), &bytes)
                        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
                });
                let document = match document {
                    Ok(document) => document,
                    Err(error) => {
                        bevy::log::warn!(%error, path = %path.display(), "could not index authored binder");
                        continue;
                    }
                };
                if document.root.name.eq_ignore_ascii_case("BFTypedBinder") {
                    if let Some(binder_type) = document.root.attribute_named_any(&["binderType"]) {
                        paths_by_type.entry(canonicalize_source_document_record_key(binder_type))
                            .or_default().push(path);
                    }
                }
            }
            paths_by_type
        });
        match declarations
            .get(&canonicalize_source_document_record_key(binder_type))
            .map(Vec::as_slice)
        {
            Some([path]) => Ok(Some(path.clone())),
            Some(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{from} references ambiguous authored binder type {binder_type}"),
            )),
            None => Ok(None),
        }
    }
}
