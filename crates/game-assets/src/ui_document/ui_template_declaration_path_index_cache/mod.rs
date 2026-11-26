//! Immutable UI template declaration indexes keyed by winning archive entries.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use z2f::ResolvedArchiveEntry;

use crate::asset_source::AssetArchives;

#[derive(Default)]
pub(super) struct UiTemplateDeclarationPathIndexCache(Mutex<UiTemplateDeclarationIndex>);

#[derive(Default)]
struct UiTemplateDeclarationIndex {
    archive_entries: Arc<[ResolvedArchiveEntry]>,
    templates: BTreeMap<PathBuf, UiTemplateDeclarations>,
    declarations: Arc<BTreeMap<String, String>>,
}

struct UiTemplateDeclarations {
    winner: ResolvedArchiveEntry,
    names: Arc<[String]>,
}

impl UiTemplateDeclarationPathIndexCache {
    pub(super) fn template_declaration_paths(
        &self,
        asset_archives: &AssetArchives,
    ) -> Arc<BTreeMap<String, String>> {
        let mut index = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            let entries = asset_archives.resolved_archive_entries();
            if Arc::ptr_eq(&index.archive_entries, &entries) {
                return Arc::clone(&index.declarations);
            }
            let mut templates = BTreeMap::new();
            let mut declarations = BTreeMap::new();
            for winner in entries.iter().filter(|entry| {
                let path = &entry.normalized_asset_path;
                path.starts_with("ui/template") && path.extension().is_some_and(|ext| ext == "xml")
            }) {
                let path = &winner.normalized_asset_path;
                let names = index.templates.get(path)
                    .filter(|cached| cached.winner == *winner)
                    .map_or_else(
                        || super::source::ui_template_source_path_discovery::read_ui_template_declarations(asset_archives, path).into(),
                        |cached| Arc::clone(&cached.names),
                    );
                if !names.is_empty() {
                    let source = path.to_string_lossy().into_owned();
                    for name in names.iter() {
                        declarations.insert(name.clone(), source.clone());
                    }
                }
                templates.insert(
                    path.clone(),
                    UiTemplateDeclarations {
                        winner: winner.clone(),
                        names,
                    },
                );
            }
            if !Arc::ptr_eq(&entries, &asset_archives.resolved_archive_entries()) {
                continue;
            }
            if index.declarations.as_ref() != &declarations {
                index.declarations = Arc::new(declarations);
            }
            index.templates = templates;
            index.archive_entries = entries;
            return Arc::clone(&index.declarations);
        }
    }
}
