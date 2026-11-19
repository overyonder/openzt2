use crate::terrain::terrain_chunk_mesh_construction::construct_terrain_chunk_mesh;
use crate::terrain::water_regions::authored_membership::TerrainWaterMembershipChanged;
use bevy::asset::AssetEvent;
use bevy::camera::visibility::RenderLayers;
use bevy::platform::collections::HashMap;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use bevy::tasks::{ComputeTaskPool, TaskPool};
use bevy::transform::helper::TransformHelper;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::material::material_asset_types::MaterialAsset;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_world_state::model_render::authored_model_material_pass_projection::{
    EffectPassMaterialRenderView, PRIMARY_EFFECT_PASS_RENDER_VIEW_LAYER,
};
use openzt2_world_state::simulation_time::native_visual_random_stream::NativeVisualRandomStream;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TerrainRenderChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk};
use openzt2_zoo_state::terrain::terrain_water_caustic_types::{
    TerrainWaterCausticAnimation, TerrainWaterCausticPass,
};
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionQuery, TerrainWaterRegionSource,
};

use super::ground_mesh::construct_caustic_ground_mesh;
use super::pass_projection::{
    reconcile_camera_specific_caustic_ground_passes, GroundCausticPassIndex,
};
use super::{
    advance_caustic_animation, bind_caustic_camera_projection, bind_caustic_textures,
    caustic_world_projection_matrices, initialize_caustic_animation,
};
use crate::terrain::terrain_change_tracking_types::TerrainDirty;

#[derive(Component)]
pub(in crate::terrain) struct TerrainCausticGroundProjectionPending;

pub(in crate::terrain) fn advance_terrain_caustic_animation(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<Assets<TerrainAsset>>,
    mut changes: MessageReader<AssetEvent<TerrainAsset>>,
    mut random: ResMut<NativeVisualRandomStream>,
    regions: TerrainWaterRegionQuery,
    authored_order: Query<Ref<TerrainWaterRegionSource>>,
    mut animations: Query<&mut TerrainWaterCausticAnimation>,
) {
    let modified: HashSet<_> = changes
        .read()
        .filter_map(|event| match event {
            AssetEvent::Modified { id } => Some(*id),
            _ => None,
        })
        .collect();
    let mut ordered = regions
        .iter()
        .map(|(entity, _)| {
            (
                authored_order
                    .get(entity)
                    .map_or(usize::MAX, |source| source.source_region_index()),
                entity,
            )
        })
        .collect::<Vec<_>>();
    ordered.sort_unstable_by_key(|&(row, entity)| (row, entity.to_bits()));
    for (_, entity) in ordered {
        let Some(source) = regions.get(entity) else {
            continue;
        };
        let Some(asset) = assets.get(source.terrain()) else {
            continue;
        };
        let Some(water) = source
            .biome(asset)
            .and_then(|biome| biome.water_presentation.as_ref())
        else {
            continue;
        };
        let configuration = &water.surface_material.configuration.caustics;
        let mut draw = || random.unit_f64();
        if let Ok(mut animation) = animations.get_mut(entity) {
            if authored_order
                .get(entity)
                .is_ok_and(|source| source.is_changed())
                || modified.contains(&source.terrain().id())
            {
                *animation = initialize_caustic_animation(configuration, &mut draw);
            }
            advance_caustic_animation(&mut animation, configuration, time.delta_secs(), &mut draw);
        } else {
            let mut animation = initialize_caustic_animation(configuration, &mut draw);
            advance_caustic_animation(&mut animation, configuration, time.delta_secs(), &mut draw);
            commands.entity(entity).insert(animation);
        }
    }
}

#[derive(Component)]
pub(in crate::terrain) struct TerrainCausticGroundMeshes(Vec<(Entity, Mesh)>);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(in crate::terrain) fn prepare_changed_terrain_caustic_ground_meshes(
    mut commands: Commands,
    assets: Res<Assets<TerrainAsset>>,
    chunks: Query<(Entity, &TerrainChunk, Option<&EditedTerrainSamples>), With<TerrainRenderChunk>>,
    changed_chunks: Query<(), Or<(Added<TerrainRenderChunk>, Changed<TerrainDirty>)>>,
    indexes: Query<&TerrainWaterMembershipIndex>,
    regions: TerrainWaterRegionQuery,
    mut membership_changes: MessageReader<TerrainWaterMembershipChanged>,
    mut terrain_changes: MessageReader<AssetEvent<TerrainAsset>>,
    mut prepared: Query<&mut TerrainCausticGroundMeshes>,
) {
    let membership_changes: Vec<_> = membership_changes.read().collect();
    let changed_assets: HashSet<_> = terrain_changes
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id }
            | AssetEvent::Removed { id } => Some(id),
            AssetEvent::Unused { .. } => None,
        })
        .collect();
    let mut jobs = Vec::new();
    for (entity, chunk, edited) in &chunks {
        let id = chunk.asset.id();
        let index = indexes.iter().find(|index| index.terrain() == id);
        let (Some(index), Some(asset)) = (index, assets.get(id)) else {
            if prepared.contains(entity) {
                commands
                    .entity(entity)
                    .remove::<TerrainCausticGroundMeshes>();
            }
            continue;
        };
        let dirty_region = membership_changes.iter().any(|change| {
            change.affects_chunk(chunk)
                || (change.terrain == id
                    && change.regions.as_ref().is_some_and(|owners| {
                        owners.iter().any(|owner| {
                            let cells = u32::from(chunk.side.saturating_sub(1)).max(1);
                            index
                                .render_tiles(*owner)
                                .any(|tile| (tile * 4 / cells).as_ivec2() == chunk.source_coord)
                        })
                    }))
        });
        if prepared.contains(entity)
            && !changed_chunks.contains(entity)
            && !changed_assets.contains(&id)
            && !dirty_region
        {
            continue;
        }
        let mut owners = Vec::new();
        for (owner, source) in regions.iter() {
            if source.terrain().id() != id {
                continue;
            }
            let cells = u32::from(chunk.side.saturating_sub(1)).max(1);
            if !index
                .render_tiles(owner)
                .any(|tile| (tile * 4 / cells).as_ivec2() == chunk.source_coord)
            {
                continue;
            }
            if let Some(water) = source
                .biome(asset)
                .and_then(|biome| biome.water_presentation.as_ref())
            {
                owners.push((
                    owner,
                    water
                        .surface_material
                        .configuration
                        .caustics
                        .terrain_tint_rgba,
                ));
            }
        }
        jobs.push((entity, chunk, edited, asset, index, owners));
    }
    let results: Vec<_> = ComputeTaskPool::get_or_init(TaskPool::default).scope(|scope| {
        for (entity, chunk, edited, asset, index, owners) in jobs {
            scope.spawn(async move {
                let mut meshes = Vec::new();
                if !owners.is_empty() {
                    let ground = construct_terrain_chunk_mesh(chunk, asset, edited)?;
                    let width = asset.canonical_terrain_grid().width;
                    for (owner, tint) in owners {
                        let tiles: HashSet<_> = index.render_tiles(owner).collect();
                        if let Some(mesh) = construct_caustic_ground_mesh(
                            chunk,
                            asset,
                            &ground,
                            tint,
                            |point| {
                                index.point_owner(UVec2::new(
                                    point as u32 % width,
                                    point as u32 / width,
                                )) == Some(owner)
                            },
                            |tile| tiles.contains(&tile),
                        ) {
                            meshes.push((owner, mesh));
                        }
                    }
                }
                Some((entity, TerrainCausticGroundMeshes(meshes)))
            });
        }
    });
    for (entity, next) in results.into_iter().flatten() {
        if let Ok(mut current) = prepared.get_mut(entity) {
            *current = next;
        } else {
            commands.entity(entity).insert(next);
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(in crate::terrain) fn project_changed_terrain_caustic_ground_passes(
    mut commands: Commands,
    assets: Res<Assets<TerrainAsset>>,
    authored_materials: Res<Assets<MaterialAsset>>,
    graphics: Res<GraphicsSettings>,
    mut materials: ResMut<Assets<EffectPassMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut meshes: ResMut<Assets<Mesh>>,
    chunks: Query<
        (
            Entity,
            &TerrainChunk,
            Option<Ref<TerrainCausticGroundMeshes>>,
            Has<TerrainCausticGroundProjectionPending>,
            Option<&Children>,
        ),
        With<TerrainRenderChunk>,
    >,
    regions: TerrainWaterRegionQuery,
    mut draws: Query<(
        &TerrainWaterCausticPass,
        &GroundCausticPassIndex,
        &mut Mesh3d,
        &mut MeshMaterial3d<EffectPassMaterial>,
        &mut RenderLayers,
    )>,
    cameras: Query<
        (
            Entity,
            Ref<Camera>,
            Option<Ref<EffectPassMaterialRenderView>>,
            Has<ZooCamera>,
        ),
        With<Camera3d>,
    >,
    mut removed_views: RemovedComponents<EffectPassMaterialRenderView>,
) {
    let new_views = cameras.iter().any(|(_, camera, view, _)| {
        camera.is_added() || view.is_some_and(|view| view.is_changed())
    }) | (removed_views.read().count() > 0);
    let views: Vec<_> = cameras
        .iter()
        .filter_map(|(entity, _, view, primary)| {
            view.map(|view| (entity, view.render_layer))
                .or_else(|| primary.then_some((entity, PRIMARY_EFFECT_PASS_RENDER_VIEW_LAYER)))
        })
        .collect();
    for (entity, chunk, prepared, pending, children) in &chunks {
        let mut existing: HashMap<_, _> = children
            .into_iter()
            .flat_map(|children| children.iter())
            .filter_map(|child| {
                let (pass, index, ..) = draws.get(child).ok()?;
                Some(((pass.water_owner, pass.render_view, index.0), child))
            })
            .collect();
        let Some(prepared) = prepared else {
            for draw in existing.into_values() {
                commands.entity(draw).try_despawn();
            }
            continue;
        };
        if !pending
            && !prepared.is_changed()
            && !graphics.is_changed()
            && !new_views
            && existing.keys().all(|(owner, view, _)| {
                regions.get(*owner).is_some() && views.iter().any(|(camera, _)| camera == view)
            })
        {
            continue;
        }
        if views.is_empty() {
            for draw in existing.into_values() {
                commands.entity(draw).try_despawn();
            }
            continue;
        }
        if !pending {
            commands
                .entity(entity)
                .insert(TerrainCausticGroundProjectionPending);
        }
        if prepared.0.is_empty() {
            for draw in existing.into_values() {
                commands.entity(draw).try_despawn();
            }
            commands
                .entity(entity)
                .remove::<TerrainCausticGroundProjectionPending>();
            continue;
        }
        let Some(asset) = assets.get(&chunk.asset) else {
            continue;
        };
        let Some(material) = asset
            .water_material("Materials/causticterrain.bfmat")
            .and_then(|handle| authored_materials.get(handle))
        else {
            continue;
        };
        let Some(templates) = material
            .evaluated_pass_material_assets_for_effect_quality(graphics.effects)
            .iter()
            .map(|handle| materials.get(handle).cloned())
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let mut ready = true;
        for (owner, mesh) in &prepared.0 {
            let Some(source) = regions.get(*owner) else {
                continue;
            };
            let Some(water) = source
                .biome(asset)
                .and_then(|biome| biome.water_presentation.as_ref())
            else {
                continue;
            };
            let mut passes = templates.clone();
            if !passes.iter_mut().all(|pass| {
                bind_caustic_textures(pass, asset, &water.surface_material.configuration.caustics)
            }) {
                ready = false;
                continue;
            }
            reconcile_camera_specific_caustic_ground_passes(
                &mut commands,
                entity,
                *owner,
                meshes.add(mesh.clone()),
                &passes,
                &views,
                &mut materials,
                &mut buffers,
                &mut existing,
                &mut draws,
            );
        }
        if ready {
            for draw in existing.into_values() {
                commands.entity(draw).try_despawn();
            }
            commands
                .entity(entity)
                .remove::<TerrainCausticGroundProjectionPending>();
        }
    }
}

pub(in crate::terrain) fn update_terrain_caustic_view_bindings(
    assets: Res<Assets<TerrainAsset>>,
    regions: TerrainWaterRegionQuery,
    animations: Query<&TerrainWaterCausticAnimation>,
    indexes: Query<&TerrainWaterMembershipIndex>,
    cameras: Query<&Camera>,
    transforms: TransformHelper,
    mut materials: ResMut<Assets<EffectPassMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut passes: Query<(
        &TerrainWaterCausticPass,
        &MeshMaterial3d<EffectPassMaterial>,
        &mut Visibility,
    )>,
) {
    let source_from_world = super::source_from_bevy_world();
    for (pass, material, mut visibility) in &mut passes {
        let visible = (|| {
            let source = regions.get(pass.water_owner)?;
            let animation = animations.get(pass.water_owner).ok()?;
            if !indexes
                .iter()
                .any(|index| index.terrain() == source.terrain().id())
            {
                return None;
            }
            let asset = assets.get(source.terrain())?;
            let water = source
                .biome(asset)
                .and_then(|biome| biome.water_presentation.as_ref())?;
            let height = source.surface_height(asset)?;
            if !cameras
                .get(pass.render_view)
                .is_ok_and(|camera| camera.is_active)
            {
                return None;
            }
            let camera = transforms.compute_global_transform(pass.render_view).ok()?;
            let mut material = materials.get_mut(&material.0)?;
            let projection = caustic_world_projection_matrices(
                animation,
                &water.surface_material.configuration.caustics,
                height,
            );
            match bind_caustic_camera_projection(
                &mut material,
                projection,
                source_from_world * camera.to_matrix(),
                &mut buffers,
            ) {
                Ok(bound) => bound.then_some(()),
                Err(error) => {
                    error!(%error, "caustic texture projection evaluation failed");
                    None
                }
            }
        })()
        .is_some();
        visibility.set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}
