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
    archive_order: Option<(usize, usize)>,
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
                        let archive_order = archives.resolved_winning_entry(&path)
                            .filter(|winner| winner.normalized_asset_path == path)
                            .map(|winner| (winner.archive_priority_index, winner.zip_entry_ordinal));
                        paths_by_type.entry(canonicalize_source_document_record_key(binder_type))
                            .or_default().push(BinderDeclaration { path, reference_types, archive_order });
                    }
                }
            }
            for declarations in paths_by_type.values_mut() {
                declarations.sort_unstable_by(|left, right| left.archive_order.cmp(&right.archive_order).then_with(|| left.path.cmp(&right.path)));
            }
            paths_by_type
        }))
    }

    pub(crate) fn animal_binder_paths(
        &self,
        archives: &AssetArchives,
    ) -> io::Result<BTreeSet<PathBuf>> {
        let declarations = self.declarations(archives)?;
        #[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
        enum BinderIdentity {
            Registered(String),
            File(PathBuf),
        }

        let mut pending = declarations
            .iter()
            .filter_map(|(key, sources)| {
                let source = sources.last()?;
                Some((
                    BinderIdentity::Registered(key.clone()),
                    source.path.clone(),
                    source.reference_types.clone(),
                ))
            })
            .collect::<VecDeque<_>>();
        let mut files = BTreeSet::new();
        let mut dependents = BTreeMap::<BinderIdentity, BTreeSet<BinderIdentity>>::new();
        while let Some((identity, path, references)) = pending.pop_front() {
            let from = path.to_string_lossy();
            for reference in references {
                let ancestor = if is_authored_file_reference(&reference) {
                    let Some(path) = resolve_binder_file_reference(archives, &from, &reference)
                    else {
                        continue;
                    };
                    let ancestor = BinderIdentity::File(path.clone());
                    if files.insert(path.clone()) {
                        match archives.read_source(&path).and_then(|bytes| {
                            parse_blue_fang_source_document(
                                AssetPath::new(path.to_string_lossy()),
                                &bytes,
                            )
                            .map_err(io::Error::other)
                        }) {
                            Ok(document) => {
                                let references = crate::species::source::collect_authored_species_type_source_references(&document);
                                pending.push_back((ancestor.clone(), path, references));
                            }
                            Err(error) => {
                                bevy::log::warn!(%error, path = %path.display(), "could not index referenced binder")
                            }
                        }
                    }
                    ancestor
                } else {
                    BinderIdentity::Registered(canonicalize_source_document_record_key(&reference))
                };
                dependents
                    .entry(ancestor)
                    .or_default()
                    .insert(identity.clone());
            }
        }

        let animal = BinderIdentity::Registered("animal".to_owned());
        let mut pending = VecDeque::from([animal.clone()]);
        let mut animal_types = BTreeSet::from([animal]);
        while let Some(ancestor) = pending.pop_front() {
            for dependent in dependents.get(&ancestor).into_iter().flatten() {
                if animal_types.insert(dependent.clone()) {
                    pending.push_back(dependent.clone());
                }
            }
        }
        Ok(animal_types
            .into_iter()
            .filter_map(|identity| {
                let BinderIdentity::Registered(key) = identity else {
                    return None;
                };
                declarations
                    .get(&key)?
                    .last()
                    .map(|declaration| declaration.path.clone())
            })
            .collect())
    }

    pub(crate) fn resolve_binder_type_source_path(
        &self,
        archives: &AssetArchives,
        binder_type: &str,
    ) -> io::Result<Option<PathBuf>> {
        let declarations = self.declarations(archives)?;
        Ok(declarations
            .get(&canonicalize_source_document_record_key(binder_type))
            .and_then(|sources| sources.last())
            .map(|declaration| declaration.path.clone()))
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
