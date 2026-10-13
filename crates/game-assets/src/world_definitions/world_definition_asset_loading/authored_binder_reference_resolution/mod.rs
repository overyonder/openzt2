use std::{collections::BTreeMap, io, path::PathBuf, sync::Mutex};

use crate::{
    asset_source::AssetArchives,
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
        source_document_semantic_name::canonicalize_source_document_record_key,
    },
};

use super::super::world_definition_asset_set_refresh_and_indexing::game_context_binder_directory_loading::load_game_context_binder_directories;

#[derive(Default)]
pub(super) struct AuthoredBinderSourcePathIndex {
    resolved: Mutex<Option<ResolvedAuthoredBinderSourcePaths>>,
}

struct ResolvedAuthoredBinderSourcePaths {
    revision: u64,
    paths_by_type: BTreeMap<String, Vec<PathBuf>>,
}

impl AuthoredBinderSourcePathIndex {
    pub(super) fn resolve_binder_type_source_path(
        &self,
        archives: &AssetArchives,
        from: &str,
        binder_type: &str,
    ) -> io::Result<(PathBuf, u64)> {
        let revision = archives.revision();
        let mut resolved = self
            .resolved
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if resolved
            .as_ref()
            .is_none_or(|index| index.revision != revision)
        {
            let mut paths_by_type = BTreeMap::<String, Vec<PathBuf>>::new();
            for directory in load_game_context_binder_directories(archives)? {
                for path in archives
                    .source_paths_under(&directory)
                    .into_iter()
                    .filter(|path| {
                        path.parent() == Some(directory.as_path())
                            && path
                                .extension()
                                .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
                    })
                {
                    // Retain only source paths. Each parsed winning binder is dropped
                    // before reading the next; the archive byte cache remains bounded.
                    let bytes = archives.read_source(&path)?;
                    let document = parse_blue_fang_source_document(
                        AssetPath::new(&path.to_string_lossy()),
                        &bytes,
                    )
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                    if !document.root.name.eq_ignore_ascii_case("BFTypedBinder") {
                        continue;
                    }
                    if let Some(binder_type) = document.root.attribute_named_any(&["binderType"]) {
                        paths_by_type
                            .entry(canonicalize_source_document_record_key(binder_type))
                            .or_default()
                            .push(path);
                    }
                }
            }
            *resolved = Some(ResolvedAuthoredBinderSourcePaths {
                revision,
                paths_by_type,
            });
        }
        let paths = resolved.as_ref().and_then(|index| {
            index
                .paths_by_type
                .get(&canonicalize_source_document_record_key(binder_type))
        });
        match paths.map(Vec::as_slice) {
            Some([path]) => Ok((path.clone(), revision)),
            Some(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{from} references ambiguous authored binder type {binder_type}"),
            )),
            None => Err(super::unresolved_reference(
                from,
                "authored binder type",
                binder_type,
            )),
        }
    }
}
