//! Archive-revision-aware cache of winning UI template declaration paths.

use std::{collections::BTreeMap, sync::Mutex};

#[derive(Default)]
pub(super) struct UiTemplateDeclarationPathIndexCache {
    archive_revision_and_template_paths: Mutex<(Option<u64>, BTreeMap<String, String>)>,
}

impl UiTemplateDeclarationPathIndexCache {
    pub(super) fn template_declaration_paths_for_current_archive_revision(
        &self,
        asset_archives: &crate::asset_source::AssetArchives,
    ) -> BTreeMap<String, String> {
        let archive_revision = asset_archives.revision();
        let mut cached_index = self
            .archive_revision_and_template_paths
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if cached_index.0 != Some(archive_revision) {
            *cached_index = (
                Some(archive_revision),
                super::source::ui_template_source_path_discovery::build_ui_template_declaration_source_path_index(
                    asset_archives,
                ),
            );
        }
        cached_index.1.clone()
    }
}
