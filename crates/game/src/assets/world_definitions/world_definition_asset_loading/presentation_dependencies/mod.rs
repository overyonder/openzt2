use super::super::world_definition_document_asset_and_demand_loaded_dependency_paths::{
    DemandLoadedDependencyPath, WorldDefinitionAsset,
    AUTHORED_GROUND_PATH_SURFACE_MASK_SOURCE_PATHS,
};
use super::unresolved_reference;
use crate::asset_source::AssetArchives;
use crate::assets::model_source::native_model_source_lowering::native_model_scene_labelled_asset_path;
use crate::assets::source_document::{
    blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex,
    blue_fang_source_dependency_reference_discovery::{
        collect_blue_fang_actor_manifest_source_references,
        collect_blue_fang_native_model_source_references,
        collect_blue_fang_static_asset_source_references,
    },
    ordered_source_document_types::OrderedSourceDocument,
};
use crate::assets::texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image;
use bevy::asset::LoadContext;
use openzt2_game_data::{world_definitions::document::WorldDefinitionDocument, AssetId};
use std::{collections::BTreeMap, io, path::Path};

pub(super) async fn resolve_world_definition_presentation_models(
    archives: &AssetArchives,
    context: &mut LoadContext<'_>,
    path: &str,
    documents: &[OrderedSourceDocument],
) -> io::Result<(
    BlueFangActorManifestModelAndSceneResolutionIndex,
    BTreeMap<(String, String), String>,
)> {
    let mut actor_manifest_resolution_index =
        BlueFangActorManifestModelAndSceneResolutionIndex::default();
    let mut manifests = collect_blue_fang_actor_manifest_source_references(documents);
    while let Some(manifest) = manifests.pop_first() {
        let resolved = archives
            .resolve_source_reference(Path::new(path), &manifest)
            .ok_or_else(|| unresolved_reference(path, "actor manifest", &manifest))?;
        let manifest = resolved.to_string_lossy().replace('\\', "/");
        let actor_manifest =
            super::source_documents::read_source_document(context, &manifest).await?;
        actor_manifest_resolution_index
            .insert_parsed_blue_fang_actor_model_manifest(&actor_manifest);
    }

    let referenced_models = collect_blue_fang_native_model_source_references(
        documents,
        &actor_manifest_resolution_index,
    );
    let mut resolved_models = BTreeMap::new();
    for (declaring_source, model, prefer_blue_fang_bfb) in &referenced_models {
        let Some(resolved) = archives.resolve_model_reference_with_blue_fang_bfb_preference(
            Path::new(declaring_source),
            model,
            *prefer_blue_fang_bfb,
        ) else {
            // Required scenes fail during lowering; optional references warn
            // when their presentation dependencies are registered.
            continue;
        };
        let resolved = resolved.to_string_lossy().replace('\\', "/");
        resolved_models.insert((declaring_source.clone(), model.clone()), resolved);
    }
    for ((_, native_model), resolved) in &resolved_models {
        actor_manifest_resolution_index.register_archive_resolved_native_model_scene_asset_path(
            native_model,
            &native_model_scene_labelled_asset_path(resolved),
        );
    }

    Ok((actor_manifest_resolution_index, resolved_models))
}

pub(super) fn register_world_definition_presentation_dependencies(
    archives: &AssetArchives,
    path: &str,
    documents: &[OrderedSourceDocument],
    document: WorldDefinitionDocument,
    actor_manifest_resolution_index: &BlueFangActorManifestModelAndSceneResolutionIndex,
    resolved_models: &BTreeMap<(String, String), String>,
) -> io::Result<WorldDefinitionAsset> {
    let mut textures = Vec::new();
    let mut models = Vec::new();
    let mut effects = Vec::new();
    let mut scenes = Vec::new();
    let mut animation_sets = actor_manifest_resolution_index
        .animation_set_asset_paths_by_actor_manifest_key()
        .iter()
        .map(
            |(authored_path, resolved_path)| DemandLoadedDependencyPath {
                id: AssetId::from_virtual_path(authored_path),
                path: resolved_path.clone().into_boxed_str(),
            },
        )
        .collect::<Vec<_>>();
    for ((_, authored), resolved) in resolved_models {
        let resolved_scene = native_model_scene_labelled_asset_path(resolved);
        models.push(DemandLoadedDependencyPath {
            id: AssetId::from_virtual_path(resolved),
            path: resolved.clone().into_boxed_str(),
        });
        models.push(DemandLoadedDependencyPath {
            id: AssetId::from_virtual_path(authored),
            path: resolved.clone().into_boxed_str(),
        });
        scenes.push(DemandLoadedDependencyPath {
            id: AssetId::from_virtual_path(&resolved_scene),
            path: resolved_scene.clone().into_boxed_str(),
        });
        scenes.push(DemandLoadedDependencyPath {
            id: AssetId::from_virtual_path(authored),
            path: resolved_scene.into_boxed_str(),
        });
    }
    for (declaring_source, referenced) in
        collect_blue_fang_static_asset_source_references(documents)
    {
        let id = AssetId::from_virtual_path(&referenced);
        match referenced.rsplit_once('.').map(|(_, extension)| extension) {
            Some(extension)
                if ["dds", "png", "jpg", "jpeg", "bmp", "tga"]
                    .iter()
                    .any(|candidate| extension.eq_ignore_ascii_case(candidate)) =>
            {
                let Some(resolved) =
                    archives.resolve_ui_reference(Path::new(&declaring_source), &referenced)
                else {
                    bevy::log::warn!(
                        source = %declaring_source,
                        texture = %referenced,
                        "world definition references a missing presentation texture"
                    );
                    continue;
                };
                textures.push(DemandLoadedDependencyPath {
                    id,
                    path: select_bevy_image_asset_path_for_blue_fang_source_image(
                        &resolved.to_string_lossy().replace('\\', "/"),
                    )
                    .into_boxed_str(),
                });
            }
            Some(extension)
                if extension.eq_ignore_ascii_case("nif")
                    || extension.eq_ignore_ascii_case("bfb") =>
            {
                let scene_path = native_model_scene_labelled_asset_path(&referenced);
                let Some(model_path) =
                    archives.resolve_model_reference(Path::new(&declaring_source), &referenced)
                else {
                    bevy::log::warn!(
                        source = %declaring_source,
                        model = %referenced,
                        "world definition references a missing presentation model"
                    );
                    continue;
                };
                let model_path = model_path.to_string_lossy().replace('\\', "/");
                let resolved_scene_path = native_model_scene_labelled_asset_path(&model_path);
                models.push(DemandLoadedDependencyPath {
                    id: AssetId::from_virtual_path(&model_path),
                    path: model_path.into_boxed_str(),
                });
                scenes.push(DemandLoadedDependencyPath {
                    id: AssetId::from_virtual_path(&scene_path),
                    path: resolved_scene_path.into_boxed_str(),
                });
            }
            Some(extension) if extension.eq_ignore_ascii_case("psys") => {
                let resolved = archives
                    .resolve_source_reference(Path::new(&declaring_source), &referenced)
                    .ok_or_else(|| {
                        unresolved_reference(&declaring_source, "particle effect", &referenced)
                    })?
                    .to_string_lossy()
                    .replace('\\', "/");
                effects.push(DemandLoadedDependencyPath {
                    id,
                    path: resolved.into_boxed_str(),
                });
            }
            _ => {}
        }
    }
    if document.paths.iter().any(|definition| !definition.elevated) {
        append_authored_ground_path_surface_mask_texture_dependencies(
            archives,
            path,
            &mut textures,
        );
    }
    sort_paths(&mut textures);
    sort_paths(&mut models);
    sort_paths(&mut effects);
    sort_paths(&mut scenes);
    sort_paths(&mut animation_sets);
    Ok(WorldDefinitionAsset {
        document,
        textures: textures.into_boxed_slice(),
        models: models.into_boxed_slice(),
        effects: effects.into_boxed_slice(),
        scenes: scenes.into_boxed_slice(),
        animation_sets: animation_sets.into_boxed_slice(),
    })
}
/// Registers the detail masks used when rendering ground paths.
fn append_authored_ground_path_surface_mask_texture_dependencies(
    archives: &AssetArchives,
    declaring_source: &str,
    textures: &mut Vec<DemandLoadedDependencyPath>,
) {
    textures.extend(
        AUTHORED_GROUND_PATH_SURFACE_MASK_SOURCE_PATHS
            .into_iter()
            .filter_map(|source_path| {
                let resolved =
                    archives.resolve_ui_reference(Path::new(declaring_source), source_path)?;
                Some(DemandLoadedDependencyPath {
                    id: AssetId::from_virtual_path(source_path),
                    path: select_bevy_image_asset_path_for_blue_fang_source_image(
                        &resolved.to_string_lossy().replace('\\', "/"),
                    )
                    .into_boxed_str(),
                })
            }),
    );
}

fn sort_paths(paths: &mut Vec<DemandLoadedDependencyPath>) {
    paths.sort_unstable_by_key(|path| path.id);
    paths.dedup_by_key(|path| path.id);
}
