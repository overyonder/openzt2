//! Last-winner model and scene resolution for parsed Blue Fang actor manifests.

use std::collections::BTreeMap;
use std::{
    collections::BTreeSet,
    io,
    path::{Path, PathBuf},
};

use bevy::asset::LoadContext;

use crate::asset_source::AssetArchives;

use super::{
    blue_fang_source_document_format::BlueFangSourceDocumentFormat,
    blue_fang_source_document_parsing::parse_blue_fang_source_document,
    ordered_source_document_types::OrderedSourceDocument,
    path::{authored_source_reference_paths, AssetPath},
    source_document_semantic_name::source_document_names_are_semantically_equal,
};

#[derive(Default)]
pub struct BlueFangActorManifestModelAndSceneResolutionIndex {
    native_model_source_paths_by_actor_manifest_key: BTreeMap<String, String>,
    animation_set_asset_paths_by_actor_manifest_key: BTreeMap<String, String>,
    scene_asset_paths_by_authored_actor_key: BTreeMap<String, String>,
    scoped_actor_manifests: BTreeMap<(String, String), String>,
}

impl BlueFangActorManifestModelAndSceneResolutionIndex {
    pub(crate) async fn load(
        archives: &AssetArchives,
        context: &mut LoadContext<'_>,
        documents: &[OrderedSourceDocument],
    ) -> io::Result<Self> {
        let mut index = Self::default();
        let mut loaded = BTreeSet::new();
        for document in documents {
            let declaring = document.path.key();
            let mut nodes = vec![&document.root];
            while let Some(node) = nodes.pop() {
                nodes.extend(node.element_children());
                if source_document_names_are_semantically_equal(
                    &node.name,
                    "BFTravAnimPathComponent",
                ) {
                    continue;
                }
                let Some(reference) = node
                    .attribute_named_any(&["actorfile"])
                    .filter(|value| !value.trim().is_empty())
                else {
                    continue;
                };
                let resolved = authored_source_reference_paths(&declaring, reference)
                    .map(|path| PathBuf::from(path.key()))
                    .find(|path| archives.source_exists(path))
                    .or_else(|| archives.resolve_source_reference(Path::new(&declaring), reference))
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::NotFound,
                            format!("{declaring} references missing actor manifest {reference}"),
                        )
                    })?;
                let manifest_path = AssetPath::new(resolved.to_string_lossy()).key();
                if loaded.insert(manifest_path.clone()) {
                    let bytes = context
                        .read_asset_bytes(resolved)
                        .await
                        .map_err(io::Error::other)?;
                    let model = if manifest_path.ends_with(".kfm") {
                        Some(crate::animation::kfm_animation_set_lowering::lower_kfm_source_to_animation_set_document(&manifest_path, &bytes)?.model_asset_path)
                    } else {
                        let manifest =
                            parse_blue_fang_source_document(AssetPath::new(&manifest_path), &bytes)
                                .map_err(|error| {
                                    io::Error::new(io::ErrorKind::InvalidData, error)
                                })?;
                        if manifest.format == BlueFangSourceDocumentFormat::BlueFangModelManifest {
                            manifest
                                .root
                                .element_children()
                                .find(|node| node.name.eq_ignore_ascii_case("BFM"))
                                .and_then(|node| node.attribute("modelname"))
                                .map(str::to_owned)
                        } else {
                            None
                        }
                    };
                    if let Some(reference) = model {
                        let model = authored_source_reference_paths(&manifest_path, &reference)
                            .map(|path| PathBuf::from(path.key()))
                            .find(|path| archives.source_exists(path))
                            .or_else(|| {
                                archives
                                    .resolve_model_reference(Path::new(&manifest_path), &reference)
                            })
                            .map_or_else(
                                || AssetPath::new(&reference).key(),
                                |path| AssetPath::new(path.to_string_lossy()).key(),
                            );
                        index
                            .native_model_source_paths_by_actor_manifest_key
                            .insert(manifest_path.clone(), model.clone());
                        index.scene_asset_paths_by_authored_actor_key.insert(
                            manifest_path.clone(),
                            crate::model_source::native_model_source_lowering::native_model_scene_labelled_asset_path(&model),
                        );
                    }
                }
                if index
                    .native_model_source_paths_by_actor_manifest_key
                    .contains_key(&manifest_path)
                {
                    index
                        .animation_set_asset_paths_by_actor_manifest_key
                        .insert(manifest_path.clone(), manifest_path.clone());
                    index
                        .animation_set_asset_paths_by_actor_manifest_key
                        .insert(AssetPath::new(reference).key(), manifest_path.clone());
                }
                if let Some(scene) = index
                    .scene_asset_paths_by_authored_actor_key
                    .get(&manifest_path)
                    .cloned()
                {
                    index
                        .scene_asset_paths_by_authored_actor_key
                        .insert(AssetPath::new(reference).key(), scene);
                }
                index
                    .scoped_actor_manifests
                    .insert((declaring.clone(), reference.to_owned()), manifest_path);
            }
        }
        Ok(index)
    }

    pub(crate) fn scoped_actor_manifest_key(
        &self,
        declaring_path: &str,
        reference: &str,
    ) -> String {
        self.scoped_actor_manifests
            .get(&(declaring_path.to_owned(), reference.to_owned()))
            .cloned()
            .unwrap_or_else(|| AssetPath::new(reference).key())
    }

    pub fn register_archive_resolved_actor_scene_asset_path(
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
    pub fn register_archive_resolved_native_model_scene_asset_path(
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

    pub fn actor_scene_asset_paths_by_authored_actor_key(&self) -> &BTreeMap<String, String> {
        &self.scene_asset_paths_by_authored_actor_key
    }

    pub fn animation_set_asset_paths_by_actor_manifest_key(&self) -> &BTreeMap<String, String> {
        &self.animation_set_asset_paths_by_actor_manifest_key
    }

    pub fn native_model_source_path_for_normalized_actor_manifest_key(
        &self,
        normalized_actor_manifest_key: &str,
    ) -> Option<&str> {
        self.native_model_source_paths_by_actor_manifest_key
            .get(normalized_actor_manifest_key)
            .map(String::as_str)
    }

    pub fn native_model_source_paths_in_actor_manifest_key_order(
        &self,
    ) -> impl Iterator<Item = &str> {
        self.native_model_source_paths_by_actor_manifest_key
            .values()
            .map(String::as_str)
    }
}
