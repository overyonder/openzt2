use bevy::asset::AssetEvent;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
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
    TerrainWaterCausticAnimation, TerrainWaterCausticPass, TerrainWaterCausticProjectionRevision,
};
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionQuery, TerrainWaterRegionSource,
};

use super::ground_mesh::construct_caustic_ground_mesh;
use super::pass_projection::spawn_camera_specific_caustic_ground_passes;
use super::{
    advance_caustic_animation, bind_caustic_camera_projection, bind_caustic_textures,
    caustic_world_projection_matrices, initialize_caustic_animation,
};
use crate::terrain::terrain_change_tracking_types::TerrainDirty;

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
            Option<&EditedTerrainSamples>,
            Option<&TerrainDirty>,
            Option<&TerrainWaterCausticProjectionRevision>,
            Option<&Children>,
        ),
        With<TerrainRenderChunk>,
    >,
    indexes: Query<Ref<TerrainWaterMembershipIndex>>,
    regions: TerrainWaterRegionQuery,
    caustic_passes: Query<(), With<TerrainWaterCausticPass>>,
    cameras: Query<
        (
            Entity,
            Ref<Camera>,
            Option<&EffectPassMaterialRenderView>,
            Has<ZooCamera>,
        ),
        With<Camera3d>,
    >,
) {
    let render_views = cameras
        .iter()
        .filter_map(|(entity, _, view, primary)| {
            view.map(|view| (entity, view.render_layer))
                .or_else(|| primary.then_some((entity, PRIMARY_EFFECT_PASS_RENDER_VIEW_LAYER)))
        })
        .collect::<Vec<_>>();
    let new_camera = cameras.iter().any(|(_, camera, _, _)| camera.is_added());
    for (chunk_entity, chunk, edited, dirty, revision, children) in &chunks {
        let membership = indexes
            .iter()
            .find(|index| index.terrain() == chunk.asset.id());
        let target_revision = dirty.map_or(0, |dirty| dirty.revision);
        let unchanged = membership.as_ref().is_some_and(|index| !index.is_changed())
            && revision.is_some_and(|revision| revision.0 >= target_revision)
            && !graphics.is_changed()
            && !assets.is_changed()
            && !new_camera;
        if unchanged {
            continue;
        }
        for child in children.into_iter().flat_map(|children| children.iter()) {
            if caustic_passes.contains(child) {
                commands.entity(child).despawn();
            }
        }
        commands
            .entity(chunk_entity)
            .remove::<TerrainWaterCausticProjectionRevision>();
        // An absent index is unresolved; no saved membership is substituted.
        let (Some(index), Some(asset)) = (membership, assets.get(&chunk.asset)) else {
            continue;
        };
        if render_views.is_empty() {
            continue;
        }
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
        let mut prepared = Vec::new();
        let mut ready = true;
        for (owner, source) in regions.iter() {
            if source.terrain().id() != chunk.asset.id() {
                continue;
            }
            let Some(water) = source
                .biome(asset)
                .and_then(|biome| biome.water_presentation.as_ref())
            else {
                ready = false;
                break;
            };
            let configuration = &water.surface_material.configuration.caustics;
            let tiles: HashSet<_> = index.render_tiles(owner).collect();
            let width = asset.canonical_terrain_grid().width;
            let Some(mesh) = construct_caustic_ground_mesh(
                chunk,
                asset,
                edited,
                configuration.terrain_tint_rgba,
                |point| {
                    index.point_owner(UVec2::new(point as u32 % width, point as u32 / width))
                        == Some(owner)
                },
                |tile| tiles.contains(&tile),
            ) else {
                continue;
            };
            let mut passes = templates.clone();
            if !passes
                .iter_mut()
                .all(|pass| bind_caustic_textures(pass, asset, configuration))
            {
                ready = false;
                break;
            }
            prepared.push((owner, mesh, passes));
        }
        if !ready {
            continue;
        }
        for (owner, mesh, passes) in prepared {
            spawn_camera_specific_caustic_ground_passes(
                &mut commands,
                chunk_entity,
                owner,
                meshes.add(mesh),
                &passes,
                &render_views,
                &mut materials,
                &mut buffers,
            );
        }
        commands
            .entity(chunk_entity)
            .insert(TerrainWaterCausticProjectionRevision(target_revision));
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

pub(in crate::terrain) fn despawn_caustic_passes_without_owner_or_view(
    mut commands: Commands,
    regions: TerrainWaterRegionQuery,
    cameras: Query<(), With<Camera>>,
    passes: Query<(Entity, &TerrainWaterCausticPass)>,
) {
    for (entity, pass) in &passes {
        if regions.get(pass.water_owner).is_none() || !cameras.contains(pass.render_view) {
            commands.entity(entity).despawn();
        }
    }
}
