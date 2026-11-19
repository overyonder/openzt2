//! Cache of winning UI template declaration paths for the current resolved archive paths.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Default)]
pub(super) struct UiTemplateDeclarationPathIndexCache {
    resolved_paths_and_template_paths: Mutex<(Option<Arc<[PathBuf]>>, BTreeMap<String, String>)>,
}

impl UiTemplateDeclarationPathIndexCache {
    /// Archive mutation publishes a new resolved path list, so the cached
    /// index is rebuilt whenever the list it was built from is replaced.
    pub(super) fn template_declaration_paths_for_resolved_archive_paths(
        &self,
        asset_archives: &crate::asset_source::AssetArchives,
    ) -> BTreeMap<String, String> {
        let resolved_paths = asset_archives.resolved_paths();
        let mut cached_index = self
            .resolved_paths_and_template_paths
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !cached_index
            .0
            .as_ref()
            .is_some_and(|cached| Arc::ptr_eq(cached, &resolved_paths))
        {
            *cached_index = (
                Some(Arc::clone(&resolved_paths)),
                super::source::ui_template_source_path_discovery::build_ui_template_declaration_source_path_index(
                    asset_archives,
                    &resolved_paths,
                ),
            );
        }
        cached_index.1.clone()
    }
}
