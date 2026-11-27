use super::super::lower::named_physical_presentation_source_lowering::AuthoredPhysicalActor;
use super::super::world_definition_document_asset_and_demand_loaded_dependency_paths::{
    DemandLoadedDependencyPath, WorldDefinitionAsset,
    AUTHORED_GROUND_PATH_SURFACE_MASK_SOURCE_PATHS,
};
use super::unresolved_reference;
use crate::asset_source::AssetArchives;
use crate::model_source::native_model_source_lowering::native_model_scene_labelled_asset_path;
use crate::source_document::{
    blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex,
    blue_fang_source_dependency_reference_discovery::{
        collect_blue_fang_native_model_source_references,
        collect_blue_fang_static_asset_source_references,
    },
    ordered_source_document_types::OrderedSourceDocument,
};
use crate::texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image;
use bevy::asset::LoadContext;
use openzt2_game_data::{world_definitions::document::WorldDefinitionDocument, AssetId};
use std::{collections::BTreeMap, io, path::Path};

pub(super) async fn resolve_world_definition_presentation_models(
    archives: &AssetArchives,
    context: &mut LoadContext<'_>,
    documents: &[OrderedSourceDocument],
) -> io::Result<(
    BlueFangActorManifestModelAndSceneResolutionIndex,
    BTreeMap<(String, String), String>,
)> {
    let mut actor_manifest_resolution_index =
        BlueFangActorManifestModelAndSceneResolutionIndex::load(archives, context, documents)
            .await?;

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
    tank_material_paths: std::collections::BTreeSet<String>,
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
    for source in documents {
        let source_path = source.path.key();
        let mut nodes = vec![&source.root];
        while let Some(node) = nodes.pop() {
            if node.name.eq_ignore_ascii_case("ZTGuestViewMode") {
                for attribute in ["tagFile", "groundCursor"] {
                    let Some(reference) = node
                        .attribute_named_any(&[attribute])
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                    else {
                        continue;
                    };
                    let authored = crate::source_document::path::AssetPath::new(reference).key();
                    let resolved = if Path::new(&authored)
                        .extension()
                        .is_some_and(|extension| extension == "xml")
                    {
                        archives
                            .resolve_source_reference(Path::new(&source_path), reference)
                            .map(|path| path.to_string_lossy().into_owned())
                    } else {
                        archives
                            .resolve_model_reference(Path::new(&source_path), reference)
                            .map(|path| {
                                native_model_scene_labelled_asset_path(&path.to_string_lossy())
                            })
                    }
                    .ok_or_else(|| {
                        unresolved_reference(&source_path, "Guest View scene", reference)
                    })?;
                    scenes.push(DemandLoadedDependencyPath {
                        id: AssetId::from_virtual_path(&authored),
                        path: resolved.into_boxed_str(),
                    });
                }
            }
            nodes.extend(node.element_children());
        }
    }
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
                let Some(resolved) = archives
                    .resolve_world_presentation_texture(Path::new(&declaring_source), &referenced)
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
                if crate::model_source::gltf_model_source::is_model_source_extension(extension) =>
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
    let mut materials = Vec::new();
    let mut tank_water_material_sources = Vec::new();
    for path in tank_material_paths {
        let id = AssetId::from_virtual_path(&path);
        let dependency = DemandLoadedDependencyPath {
            id,
            path: path.into_boxed_str(),
        };
        if dependency.path.ends_with(".xml") {
            tank_water_material_sources.push(dependency);
        } else {
            materials.push(dependency);
        }
    }
    sort_paths(&mut materials);
    sort_paths(&mut tank_water_material_sources);
    sort_paths(&mut textures);
    sort_paths(&mut models);
    sort_paths(&mut effects);
    sort_paths(&mut scenes);
    sort_paths(&mut animation_sets);
    Ok(WorldDefinitionAsset {
        document,
        metadata: Default::default(),
        authored_binder_sources: None,
        textures: textures.into_boxed_slice(),
        models: models.into_boxed_slice(),
        materials: materials.into_boxed_slice(),
        tank_water_material_sources: tank_water_material_sources.into_boxed_slice(),
        effects: effects.into_boxed_slice(),
        scenes: scenes.into_boxed_slice(),
        bound_scenes: BTreeMap::new(),
        animation_sets: animation_sets.into_boxed_slice(),
    })
}

pub(super) async fn bind_world_definition_physical_actors(
    context: &mut LoadContext<'_>,
    asset: &mut WorldDefinitionAsset,
    manifests: &BlueFangActorManifestModelAndSceneResolutionIndex,
    mut by_object: BTreeMap<AssetId, BTreeMap<AssetId, AuthoredPhysicalActor>>,
) -> io::Result<()> {
    for object in &mut asset.document.objects {
        let Some(binders) = by_object.remove(&object.id) else {
            continue;
        };
        for (binder, actor) in binders {
            let manifest = manifests.scoped_actor_manifest_key(&actor.source, &actor.reference);
            let model = manifests
                .native_model_source_path_for_normalized_actor_manifest_key(&manifest)
                .ok_or_else(|| {
                    unresolved_reference(&actor.source, "physical actor model", &actor.reference)
                })?;
            let bytes = context
                .read_asset_bytes(model.to_owned())
                .await
                .map_err(io::Error::other)?;
            let mut scene = crate::model_source::native_model_source_lowering::lower_native_physical_scene_document(model, &bytes)
                .map_err(io::Error::other)?;
            if let Some(distances) = asset
                .metadata
                .render_lod_distances
                .get(&(object.id, binder))
            {
                for policy in scene
                    .entities
                    .iter_mut()
                    .filter_map(|entity| entity.render_lod_policy.as_mut())
                {
                    for (defaults, overrides) in [
                        (
                            &mut policy.switch_distance_squared,
                            &distances.switch_distance_squared,
                        ),
                        (
                            &mut policy.minimum_switch_distance_squared,
                            &distances.minimum_switch_distance_squared,
                        ),
                    ] {
                        let shared = defaults.len().min(overrides.len());
                        defaults[..shared].copy_from_slice(&overrides[..shared]);
                        defaults.extend_from_slice(&overrides[shared..]);
                    }
                }
            }
            let animation_set = context
                .load::<crate::animation::animation_set_asset_types::AnimationSetAsset>(
                manifest,
            );
            let mut prefab = crate::scene_prefab::create_scene_prefab_asset_and_load_dependencies(
                scene, context,
            );
            for row in 0..prefab.canonical_scene_prefab_document().entities.len() {
                let renderable_count = prefab.canonical_scene_prefab_document().entities[row]
                    .renderables
                    .len();
                for index in 0..renderable_count {
                    prefab.bind_animation_set_to_renderable(
                        row,
                        index,
                        animation_set.clone(),
                        actor.start.clone(),
                    );
                }
            }
            let label = format!(
                "actor/{}/{}",
                object.id.to_lowercase_hexadecimal_string(),
                binder.to_lowercase_hexadecimal_string()
            );
            let id = AssetId::from_key(&label);
            asset.bound_scenes.insert(
                id,
                context.add_loaded_labeled_asset(
                    label,
                    bevy::asset::LoadedAsset::new_with_dependencies(prefab),
                ),
            );
            for row in object
                .named_physical_presentations
                .iter_mut()
                .filter(|row| row.name == binder)
            {
                row.prefab = id;
            }
            for state in object
                .presentation_attachments
                .iter_mut()
                .flat_map(|controller| &mut controller.states)
                .filter(|state| state.child_binder == binder)
            {
                state.prefab = id;
            }
        }
    }
    Ok(())
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
