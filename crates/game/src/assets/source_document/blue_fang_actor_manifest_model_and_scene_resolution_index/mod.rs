//! Last-winner model and scene resolution for parsed Blue Fang actor manifests.

use std::collections::BTreeMap;

use super::{
    blue_fang_source_document_format::BlueFangSourceDocumentFormat,
    ordered_source_document_types::OrderedSourceDocument, path::AssetPath,
};

#[derive(Default)]
pub(in crate::assets) struct BlueFangActorManifestModelAndSceneResolutionIndex {
    native_model_source_paths_by_actor_manifest_key: BTreeMap<String, String>,
    animation_set_asset_paths_by_actor_manifest_key: BTreeMap<String, String>,
    scene_asset_paths_by_authored_actor_key: BTreeMap<String, String>,
}

impl BlueFangActorManifestModelAndSceneResolutionIndex {
    pub(in crate::assets) fn insert_parsed_blue_fang_actor_model_manifest(
        &mut self,
        actor_manifest: &OrderedSourceDocument,
    ) {
        if actor_manifest.format != BlueFangSourceDocumentFormat::BlueFangModelManifest {
            return;
        }
        let Some(native_model_source_path) = actor_manifest
            .root
            .element_children()
            .find(|source_node| source_node.name.eq_ignore_ascii_case("BFM"))
            .and_then(|blue_fang_model_manifest| blue_fang_model_manifest.attribute("modelname"))
            .map(|native_model_source_path| AssetPath::new(native_model_source_path).key())
        else {
            return;
        };
        let actor_manifest_key = actor_manifest.path.key();
        self.animation_set_asset_paths_by_actor_manifest_key.insert(
            actor_manifest_key.clone(),
            actor_manifest.path.as_str().to_owned(),
        );
        self.scene_asset_paths_by_authored_actor_key.insert(
            actor_manifest_key.clone(),
            crate::assets::model_source::native_model_source_lowering::
                native_model_scene_labelled_asset_path(&native_model_source_path),
        );
        self.native_model_source_paths_by_actor_manifest_key
            .insert(actor_manifest_key, native_model_source_path);
    }

    pub(in crate::assets) fn register_archive_resolved_actor_scene_asset_path(
        &mut self,
        authored_actor_path: &str,
        resolved_scene_asset_path: String,
    ) {
        self.scene_asset_paths_by_authored_actor_key.insert(
            AssetPath::new(authored_actor_path).key(),
            resolved_scene_asset_path,
        );
    }

    /// Binds an archive-resolved model scene to the authored model path and
    /// to every actor manifest that names that model.
    pub(in crate::assets) fn register_archive_resolved_native_model_scene_asset_path(
        &mut self,
        native_model_source_path: &str,
        resolved_scene_asset_path: &str,
    ) {
        let actor_manifest_keys = self
            .native_model_source_paths_by_actor_manifest_key
            .iter()
            .filter(|(_, model)| model.as_str() == native_model_source_path)
            .map(|(actor_manifest_key, _)| actor_manifest_key.clone())
            .collect::<Vec<_>>();
        for actor_manifest_key in actor_manifest_keys {
            self.scene_asset_paths_by_authored_actor_key
                .insert(actor_manifest_key, resolved_scene_asset_path.to_owned());
        }
        self.register_archive_resolved_actor_scene_asset_path(
            native_model_source_path,
            resolved_scene_asset_path.to_owned(),
        );
    }

    pub(in crate::assets) fn actor_scene_asset_paths_by_authored_actor_key(
        &self,
    ) -> &BTreeMap<String, String> {
        &self.scene_asset_paths_by_authored_actor_key
    }

    pub(in crate::assets) fn animation_set_asset_paths_by_actor_manifest_key(
        &self,
    ) -> &BTreeMap<String, String> {
        &self.animation_set_asset_paths_by_actor_manifest_key
    }

    pub(in crate::assets) fn native_model_source_path_for_normalized_actor_manifest_key(
        &self,
        normalized_actor_manifest_key: &str,
    ) -> Option<&str> {
        self.native_model_source_paths_by_actor_manifest_key
            .get(normalized_actor_manifest_key)
            .map(String::as_str)
    }

    pub(in crate::assets) fn native_model_source_paths_in_actor_manifest_key_order(
        &self,
    ) -> impl Iterator<Item = &str> {
        self.native_model_source_paths_by_actor_manifest_key
            .values()
            .map(String::as_str)
    }
}
