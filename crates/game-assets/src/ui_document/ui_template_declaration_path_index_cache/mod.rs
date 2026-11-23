//! Immutable UI template declaration indexes keyed by winning archive entries.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use z2f::ResolvedArchiveEntry;

use crate::asset_source::AssetArchives;

#[derive(Default)]
pub(super) struct UiTemplateDeclarationPathIndexCache(Mutex<UiTemplateDeclarationIndex>);

#[derive(Default)]
struct UiTemplateDeclarationIndex {
    archive_entries: Arc<[ResolvedArchiveEntry]>,
    template_entries: Vec<ResolvedArchiveEntry>,
    declarations: Arc<BTreeMap<String, String>>,
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
            let template_entries = entries
                .iter()
                .filter(|entry| {
                    let path = &entry.normalized_asset_path;
                    path.starts_with("ui/template")
                        && path.extension().is_some_and(|ext| ext == "xml")
                })
                .cloned()
                .collect::<Vec<_>>();
            if index.template_entries != template_entries {
                let declarations =
                    super::source::ui_template_source_path_discovery::build_ui_template_declaration_source_path_index(
                        asset_archives,
                        &template_entries,
                    );
                if !Arc::ptr_eq(&entries, &asset_archives.resolved_archive_entries()) {
                    continue;
                }
                if index.declarations.as_ref() != &declarations {
                    index.declarations = Arc::new(declarations);
                }
                index.template_entries = template_entries;
            }
            index.archive_entries = entries;
            return Arc::clone(&index.declarations);
        }
    }
}
