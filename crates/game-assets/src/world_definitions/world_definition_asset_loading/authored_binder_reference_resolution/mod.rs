use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use crate::{
    asset_source::AssetArchives,
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document,
        path::{AssetPath, authored_source_reference_paths, is_authored_file_reference},
        source_document_semantic_name::canonicalize_source_document_record_key,
    },
};

#[derive(Debug)]
pub(crate) struct AuthoredBinderSources {
    directories: io::Result<BTreeSet<PathBuf>>,
    declarations: OnceLock<BTreeMap<String, Vec<BinderDeclaration>>>,
}

#[derive(Debug)]
struct BinderDeclaration {
    path: PathBuf,
    reference_types: BTreeSet<String>,
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

    fn declarations(
        &self,
        archives: &AssetArchives,
    ) -> io::Result<&BTreeMap<String, Vec<BinderDeclaration>>> {
        let directories = self
            .directories
            .as_ref()
            .map_err(|error| io::Error::new(error.kind(), error.to_string()))?;
        Ok(self.declarations.get_or_init(|| {
            let mut paths_by_type = BTreeMap::<String, Vec<BinderDeclaration>>::new();
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
                        let reference_types = crate::species::source::collect_authored_species_type_source_references(&document);
                        paths_by_type.entry(canonicalize_source_document_record_key(binder_type))
                            .or_default().push(BinderDeclaration { path, reference_types });
                    }
                }
            }
            paths_by_type
        }))
    }

    pub(crate) fn animal_binder_paths(
        &self,
        archives: &AssetArchives,
    ) -> io::Result<BTreeSet<PathBuf>> {
        let declarations = self.declarations(archives)?;
        let types_by_path = declarations
            .iter()
            .flat_map(|(key, sources)| {
                sources
                    .iter()
                    .map(move |source| (source.path.as_path(), key.as_str()))
            })
            .collect::<BTreeMap<_, _>>();
        let mut dependents = BTreeMap::<String, BTreeSet<&str>>::new();
        for (key, sources) in declarations {
            for source in sources {
                let from = source.path.to_string_lossy();
                for reference in &source.reference_types {
                    let ancestor = if is_authored_file_reference(reference) {
                        resolve_binder_file_reference(archives, &from, reference)
                            .and_then(|path| types_by_path.get(&path.as_path()).copied())
                            .map(str::to_owned)
                    } else {
                        Some(canonicalize_source_document_record_key(reference))
                    };
                    if let Some(ancestor) = ancestor {
                        dependents.entry(ancestor).or_default().insert(key);
                    }
                }
            }
        }
        let mut pending = VecDeque::from(["animal"]);
        let mut animal_types = BTreeSet::from(["animal"]);
        while let Some(ancestor) = pending.pop_front() {
            for &dependent in dependents.get(ancestor).into_iter().flatten() {
                if animal_types.insert(dependent) {
                    pending.push_back(dependent);
                }
            }
        }
        Ok(animal_types
            .into_iter()
            .filter_map(|key| declarations.get(key))
            .flatten()
            .map(|declaration| declaration.path.clone())
            .collect())
    }

    pub(crate) fn resolve_binder_type_source_path(
        &self,
        archives: &AssetArchives,
        from: &str,
        binder_type: &str,
    ) -> io::Result<Option<PathBuf>> {
        let declarations = self.declarations(archives)?;
        match declarations
            .get(&canonicalize_source_document_record_key(binder_type))
            .map(Vec::as_slice)
        {
            Some([declaration]) => Ok(Some(declaration.path.clone())),
            Some(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{from} references ambiguous authored binder type {binder_type}"),
            )),
            None => Ok(None),
        }
    }
}

pub(crate) fn resolve_binder_file_reference(
    archives: &AssetArchives,
    from: &str,
    reference: &str,
) -> Option<PathBuf> {
    authored_source_reference_paths(from, reference)
        .map(|candidate| PathBuf::from(candidate.key()))
        .find(|candidate| archives.source_exists(candidate))
}
