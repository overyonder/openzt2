use std::collections::HashMap;

use bevy::prelude::*;
use openzt2_game_data::world_definitions::biomes_locations_and_details::{
    BiomeDetailLevel, BiomeDetailSurface,
};

use crate::assets::scene_prefab::ScenePrefabAsset;
use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::settings::graphics_settings_types::EnvironmentalDetailObjectDensity;
use crate::plugins::settings::graphics_settings_types::GraphicsSettings;
use crate::plugins::world_spawn::prefab_presentation_render_tree::spawn_prefab_render_tree;

use super::{
    terrain_biome_detail_types::{TerrainDetail, TerrainDetailProjectionState},
    terrain_change_tracking_types::TerrainDirty,
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk},
    terrain_sample_grid_queries::{
        read_materialized_terrain_sample, read_terrain_sample_height_centimetres,
    },
};

pub(super) fn project_terrain_biome_details_after_source_or_setting_changes(
    mut commands: Commands,
    settings: Res<GraphicsSettings>,
    cameras: Query<
        &crate::plugins::camera::camera_runtime_state_types::OverheadRig,
        With<crate::plugins::camera::camera_runtime_state_types::ZooCamera>,
    >,
    terrain_assets: Res<Assets<TerrainAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut chunks: Query<(
        Entity,
        &TerrainChunk,
        Option<&EditedTerrainSamples>,
        Option<&Children>,
        Option<&TerrainDirty>,
        &mut TerrainDetailProjectionState,
    )>,
    details: Query<&TerrainDetail>,
) {
    let definition_assets_changed = definitions.is_changed();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(placement) = definitions.biome_detail_placement() else {
        return;
    };
    let Ok(camera) = cameras.single() else { return };
    let authored_visibility_range_m =
        (f32::from(placement.solid_distance_cm) + f32::from(placement.fade_distance_cm)) * 0.01;
    let retained_materialization_range_m = authored_visibility_range_m * 2.0;
    let level = match settings.detail_objects {
        EnvironmentalDetailObjectDensity::Off => None,
        EnvironmentalDetailObjectDensity::Medium => Some(BiomeDetailLevel::Medium),
        EnvironmentalDetailObjectDensity::High => Some(BiomeDetailLevel::High),
    };

    for (entity, chunk, edited, children, dirty, mut projection_state) in &mut chunks {
        let revision = dirty.map_or(0, |dirty| dirty.revision);
        let source_changed = projection_state.source_revision != revision
            || settings.is_changed()
            || terrain_assets.is_changed()
            || definition_assets_changed
            || active_definitions.is_changed();
        if !source_changed
            && projection_state.retained_camera_focus.is_finite()
            && projection_state
                .retained_camera_focus
                .distance(camera.focus)
                <= authored_visibility_range_m
        {
            continue;
        }
        let mut retained_detail_entities_by_placement_key = HashMap::new();
        for detail_entity in children
            .into_iter()
            .flat_map(|children| children.iter())
            .filter_map(|child| details.get(child).ok().map(|detail| (child, detail)))
        {
            if source_changed {
                commands.entity(detail_entity.0).despawn();
            } else {
                retained_detail_entities_by_placement_key
                    .insert(detail_entity.1.placement_key, detail_entity.0);
            }
        }
        let Some(level) = level.as_ref() else {
            projection_state.source_revision = revision;
            projection_state.retained_camera_focus = camera.focus;
            continue;
        };
        let Some(asset) = terrain_assets.get(&chunk.asset) else {
            continue;
        };
        let region_m = f32::from(placement.region_size_cm) * 0.01;
        if !region_m.is_finite() || region_m <= 0.0 {
            continue;
        }
        let span_m = chunk.spacing_m * f32::from(chunk.side.saturating_sub(1));
        let regions = (span_m / region_m).ceil() as u32;
        for rz in 0..regions {
            for rx in 0..regions {
                let center = Vec2::new((rx as f32 + 0.5) * region_m, (rz as f32 + 0.5) * region_m)
                    .min(Vec2::splat(span_m));
                let sample_x = (center.x / chunk.spacing_m).round() as usize;
                let sample_z = (center.y / chunk.spacing_m).round() as usize;
                let Some(sample) = read_materialized_terrain_sample(
                    chunk,
                    asset,
                    edited,
                    sample_x.min(chunk.side as usize - 1),
                    sample_z.min(chunk.side as usize - 1),
                ) else {
                    continue;
                };
                let Some((channel, _)) = sample
                    .blend
                    .iter()
                    .enumerate()
                    .max_by_key(|(channel, weight)| (**weight, std::cmp::Reverse(*channel)))
                else {
                    continue;
                };
                let Some(biome) = asset.canonical_terrain_grid().biomes.get(channel) else {
                    continue;
                };
                let biome = openzt2_game_data::AssetId::from_key(&biome.name);
                let surface = if sample.water_style != 0 {
                    BiomeDetailSurface::Shore
                } else if sample.ground_cover == 0 {
                    BiomeDetailSurface::Ground
                } else {
                    BiomeDetailSurface::Cover
                };
                let Some(policy) = definitions.find_biome(biome).and_then(|biome| {
                    biome
                        .detail_policies
                        .iter()
                        .find(|policy| policy.level == *level && policy.surface == surface)
                }) else {
                    continue;
                };
                let choices = &policy.choices;
                let total = choices
                    .iter()
                    .map(|choice| u64::from(choice.weight))
                    .sum::<u64>();
                if total == 0 {
                    continue;
                }
                for ordinal in 0..policy.density {
                    let seed = calculate_deterministic_terrain_detail_placement_hash(
                        chunk.coord,
                        rx,
                        rz,
                        u32::from(ordinal),
                    );
                    let mut ticket = seed % total;
                    let Some(choice) = choices.iter().find(|choice| {
                        let selected = ticket < u64::from(choice.weight);
                        ticket = ticket.saturating_sub(u64::from(choice.weight));
                        selected
                    }) else {
                        continue;
                    };
                    let radius_m = f32::from(placement.radius_cm) * 0.01;
                    let x = (center.x
                        + (convert_terrain_detail_placement_hash_to_unit_interval(
                            seed.rotate_left(13),
                        ) * 2.0
                            - 1.0)
                            * radius_m)
                        .clamp(0.0, span_m);
                    let z = (center.y
                        + (convert_terrain_detail_placement_hash_to_unit_interval(
                            seed.rotate_left(29),
                        ) * 2.0
                            - 1.0)
                            * radius_m)
                        .clamp(0.0, span_m);
                    if (chunk.origin + Vec2::new(x, z)).distance(camera.focus)
                        > retained_materialization_range_m
                    {
                        continue;
                    }
                    if retained_detail_entities_by_placement_key
                        .remove(&seed)
                        .is_some()
                    {
                        continue;
                    }
                    let Some(prefab_handle) = definitions.scene(choice.prefab) else {
                        continue;
                    };
                    let x_sample = (x / chunk.spacing_m).round() as usize;
                    let z_sample = (z / chunk.spacing_m).round() as usize;
                    let Some(height_cm) = read_terrain_sample_height_centimetres(
                        chunk,
                        asset,
                        edited,
                        x_sample.min(chunk.side as usize - 1),
                        z_sample.min(chunk.side as usize - 1),
                    ) else {
                        continue;
                    };
                    commands.spawn((
                        TerrainDetail {
                            placement_key: seed,
                            prefab: prefab_handle,
                            solid_m: f32::from(placement.solid_distance_cm) * 0.01,
                            fade_m: f32::from(placement.fade_distance_cm) * 0.01,
                            materialized_render_tree_root: None,
                        },
                        Transform::from_xyz(x, f32::from(height_cm) * 0.01, z).with_rotation(
                            Quat::from_rotation_y(
                                convert_terrain_detail_placement_hash_to_unit_interval(
                                    seed.rotate_left(7),
                                ) * std::f32::consts::TAU,
                            ),
                        ),
                        ChildOf(entity),
                    ));
                }
            }
        }
        for detail_entity in retained_detail_entities_by_placement_key.into_values() {
            commands.entity(detail_entity).despawn();
        }
        projection_state.source_revision = revision;
        projection_state.retained_camera_focus = camera.focus;
    }
}

fn calculate_deterministic_terrain_detail_placement_hash(
    coord: IVec2,
    x: u32,
    z: u32,
    ordinal: u32,
) -> u64 {
    let mut value = (coord.x as u32 as u64) << 32 | coord.y as u32 as u64;
    for input in [u64::from(x), u64::from(z), u64::from(ordinal)] {
        value ^= input.wrapping_add(0x9e37_79b9_7f4a_7c15);
        value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value ^= value >> 27;
    }
    value
}

fn convert_terrain_detail_placement_hash_to_unit_interval(value: u64) -> f32 {
    ((value >> 40) as u32) as f32 / (1_u32 << 24) as f32
}

pub(super) fn materialize_terrain_biome_detail_render_trees_within_overhead_camera_range(
    mut commands: Commands,
    cameras: Query<
        Ref<crate::plugins::camera::camera_runtime_state_types::OverheadRig>,
        With<crate::plugins::camera::camera_runtime_state_types::ZooCamera>,
    >,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    mut detail_queries: ParamSet<(
        Query<(), Added<TerrainDetail>>,
        Query<(Entity, &GlobalTransform, &mut TerrainDetail)>,
    )>,
) {
    let Ok(camera) = cameras.single() else { return };
    if !camera.is_changed() && !prefabs.is_changed() && detail_queries.p0().is_empty() {
        return;
    }
    for (entity, transform, mut detail) in &mut detail_queries.p1() {
        let inside_authored_visibility_range =
            transform.translation().xz().distance(camera.focus) <= detail.solid_m + detail.fade_m;
        match (
            inside_authored_visibility_range,
            detail.materialized_render_tree_root,
        ) {
            (true, None) => {
                let Some(prefab) = prefabs.get(&detail.prefab) else {
                    continue;
                };
                let (root, _, _) = spawn_prefab_render_tree(&mut commands, prefab, entity, false);
                detail.materialized_render_tree_root = Some(root);
            }
            (false, Some(root)) => {
                commands.entity(root).despawn();
                detail.materialized_render_tree_root = None;
            }
            _ => {}
        }
    }
}
