use std::collections::HashMap;

use bevy::prelude::*;
use openzt2_game_data::world_definitions::biomes_locations_and_details::BiomeDetailLevel;
use openzt2_game_data::world_definitions::biomes_locations_and_details::BiomeDetailSurface;

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::graphics_settings_types::EnvironmentalDetailObjectDensity;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_world::world_spawn::prefab_presentation_render_tree::spawn_prefab_render_tree;

use crate::terrain::terrain_biome_detail_types::TerrainDetail;
use crate::terrain::terrain_biome_detail_types::{
    TerrainDetailProjectionState, TerrainDetailRenderRoot,
};
use crate::terrain::terrain_change_tracking_types::TerrainDirty;
use crate::terrain::terrain_sample_change_calculations::TerrainRenderInvalidated;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TerrainRenderChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_sample_grid_queries::read_materialized_terrain_sample;
use openzt2_zoo_state::terrain::terrain_sample_grid_queries::read_terrain_sample_height_centimetres;

#[derive(Component)]
pub(in crate::terrain) struct TerrainBiomeDetailProjectionPending;

#[allow(clippy::type_complexity)]
pub(super) fn project_terrain_biome_details_after_source_or_setting_changes(
    mut commands: Commands,
    settings: Res<GraphicsSettings>,
    cameras: Query<
        &openzt2_interface_state::camera::camera_runtime_state_types::OverheadRig,
        With<openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera>,
    >,
    terrain_assets: Res<Assets<TerrainAsset>>,
    (definitions, active_definitions): (Res<Assets<WorldDefinitionAsset>>, Res<WorldDefinitions>),
    mut chunks: Query<(
        Entity,
        &TerrainChunk,
        Option<&EditedTerrainSamples>,
        Option<&Children>,
        Has<TerrainBiomeDetailProjectionPending>,
        &mut TerrainDetailProjectionState,
    )>,
    mut details: Query<(&mut TerrainDetail, &mut Transform, Option<&Children>)>,
    prefab_roots: Query<(), With<TerrainDetailRenderRoot>>,
    changed_chunks: Query<(), Or<(Changed<TerrainDirty>, Changed<TerrainRenderChunk>)>>,
    mut terrain_changes: MessageReader<TerrainRenderInvalidated>,
) {
    let mut dirty_rectangles = bevy::ecs::entity::EntityHashMap::<IRect>::default();
    for change in terrain_changes.read() {
        let bounds =
            IRect::from_corners(change.region.min.as_ivec2(), change.region.max.as_ivec2());
        dirty_rectangles
            .entry(change.chunk)
            .and_modify(|previous| *previous = previous.union(bounds))
            .or_insert(bounds);
    }
    let all_sources_changed = settings.is_changed()
        || terrain_assets.is_changed()
        || definitions.is_changed()
        || active_definitions.is_changed();
    let project = active_definitions
        .get(&definitions)
        .and_then(|definitions| Some((definitions, definitions.biome_detail_placement()?)));
    let (Some((definitions, placement)), Ok(camera)) = (project, cameras.single()) else {
        for (entity, _, _, _, pending, _) in &chunks {
            if !pending && (all_sources_changed || changed_chunks.contains(entity)) {
                commands
                    .entity(entity)
                    .insert(TerrainBiomeDetailProjectionPending);
            }
        }
        return;
    };
    let authored_visibility_range_m =
        (f32::from(placement.solid_distance_cm) + f32::from(placement.fade_distance_cm)) * 0.01;
    let retained_materialization_range_m = authored_visibility_range_m * 2.0;
    let level = match settings.detail_objects {
        EnvironmentalDetailObjectDensity::Off => None,
        EnvironmentalDetailObjectDensity::Medium => Some(BiomeDetailLevel::Medium),
        EnvironmentalDetailObjectDensity::High => Some(BiomeDetailLevel::High),
    };

    for (entity, chunk, edited, children, pending, mut projection_state) in &mut chunks {
        let source_changed = pending
            || projection_state.retained_camera_focus.is_none()
            || all_sources_changed
            || changed_chunks.contains(entity)
            || dirty_rectangles.contains_key(&entity);
        let camera_stable = projection_state.retained_camera_focus.is_some_and(|focus| {
            focus.is_finite() && focus.distance(camera.focus) <= authored_visibility_range_m
        });
        if !source_changed && camera_stable {
            continue;
        }
        let region_m = f32::from(placement.region_size_cm) * 0.01;
        let asset = level
            .as_ref()
            .and_then(|_| terrain_assets.get(&chunk.asset));
        if level.is_some() && (asset.is_none() || !region_m.is_finite() || region_m <= 0.0) {
            if source_changed && !pending {
                commands
                    .entity(entity)
                    .insert(TerrainBiomeDetailProjectionPending);
            }
            continue;
        }
        if pending {
            commands
                .entity(entity)
                .remove::<TerrainBiomeDetailProjectionPending>();
        }
        let span_m = chunk.spacing_m * f32::from(chunk.side.saturating_sub(1));
        let regions = (span_m / region_m).ceil() as u32;
        let region_center = |region: UVec2| {
            ((region.as_vec2() + Vec2::splat(0.5)) * region_m).min(Vec2::splat(span_m))
        };
        let bounds = dirty_rectangles
            .get(&entity)
            .filter(|_| camera_stable && !pending && !all_sources_changed && level.is_some())
            .map(|bounds| {
                let margin =
                    Vec2::splat(f32::from(placement.radius_cm) * 0.01 + chunk.spacing_m * 0.5);
                Rect::from_corners(
                    bounds.min.as_vec2() * chunk.spacing_m - margin,
                    bounds.max.as_vec2() * chunk.spacing_m + margin,
                )
            });
        let affected = |region| bounds.is_none_or(|bounds| bounds.contains(region_center(region)));
        let mut retained_detail_entities_by_placement_key = HashMap::new();
        for detail_entity in children
            .into_iter()
            .flat_map(|children| children.iter())
            .filter_map(|child| details.get(child).ok().map(|(detail, ..)| (child, detail)))
        {
            if affected(detail_entity.1.placement_region) {
                retained_detail_entities_by_placement_key
                    .insert(detail_entity.1.placement_key, detail_entity.0);
            }
        }
        let (Some(level), Some(asset)) = (level.as_ref(), asset) else {
            for detail_entity in retained_detail_entities_by_placement_key.into_values() {
                commands.entity(detail_entity).despawn();
            }
            projection_state.retained_camera_focus = Some(camera.focus);
            continue;
        };
        let min_region = bounds.map_or(UVec2::ZERO, |bounds| {
            (bounds.min / region_m - Vec2::ONE)
                .max(Vec2::ZERO)
                .as_uvec2()
        });
        let max_region = bounds.map_or(UVec2::splat(regions), |bounds| {
            let mut maximum = (bounds.max / region_m + Vec2::ONE)
                .max(Vec2::ZERO)
                .as_uvec2();
            if bounds.max.x >= span_m {
                maximum.x = regions;
            }
            if bounds.max.y >= span_m {
                maximum.y = regions;
            }
            maximum.min(UVec2::splat(regions))
        });
        for rz in min_region.y..max_region.y {
            for rx in min_region.x..max_region.x {
                let placement_region = UVec2::new(rx, rz);
                if !affected(placement_region) {
                    continue;
                }
                let center = region_center(placement_region);
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
                    let next = TerrainDetail {
                        placement_key: seed,
                        placement_region,
                        prefab: prefab_handle,
                        solid_m: f32::from(placement.solid_distance_cm) * 0.01,
                        fade_m: f32::from(placement.fade_distance_cm) * 0.01,
                    };
                    let pose = Transform::from_xyz(x, f32::from(height_cm) * 0.01, z)
                        .with_rotation(Quat::from_rotation_y(
                            convert_terrain_detail_placement_hash_to_unit_interval(
                                seed.rotate_left(7),
                            ) * std::f32::consts::TAU,
                        ));
                    if let Some(detail_entity) =
                        retained_detail_entities_by_placement_key.remove(&seed)
                    {
                        let (mut detail, mut transform, children) = details
                            .get_mut(detail_entity)
                            .expect("indexed biome detail");
                        if detail.prefab != next.prefab {
                            for root in children
                                .into_iter()
                                .flat_map(|children| children.iter())
                                .filter(|&child| prefab_roots.contains(child))
                            {
                                commands.entity(root).despawn();
                            }
                        }
                        detail.set_if_neq(next);
                        transform.set_if_neq(pose);
                    } else {
                        commands.spawn((next, pose, ChildOf(entity)));
                    }
                }
            }
        }
        for detail_entity in retained_detail_entities_by_placement_key.into_values() {
            commands.entity(detail_entity).despawn();
        }
        if bounds.is_none() {
            projection_state.retained_camera_focus = Some(camera.focus);
        }
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
        Ref<openzt2_interface_state::camera::camera_runtime_state_types::OverheadRig>,
        With<openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera>,
    >,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    mut prefab_events: MessageReader<AssetEvent<ScenePrefabAsset>>,
    details: Query<(
        Entity,
        Ref<TerrainDetail>,
        Ref<Transform>,
        &ChildOf,
        Option<&Children>,
    )>,
    changed_details: Query<
        (),
        (
            With<TerrainDetail>,
            Or<(Changed<TerrainDetail>, Changed<Transform>)>,
        ),
    >,
    chunks: Query<&TerrainChunk>,
    prefab_roots: Query<(), With<TerrainDetailRenderRoot>>,
) {
    let Ok(camera) = cameras.single() else { return };
    let ready_assets: bevy::platform::collections::HashSet<_> = prefab_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::LoadedWithDependencies { id } => Some(id),
            _ => None,
        })
        .collect();
    if !camera.is_changed() && ready_assets.is_empty() && changed_details.is_empty() {
        return;
    }
    for (entity, detail, transform, parent, children) in &details {
        if !camera.is_changed()
            && !detail.is_changed()
            && !transform.is_changed()
            && !ready_assets.contains(&detail.prefab.id())
        {
            continue;
        }
        let Ok(chunk) = chunks.get(parent.parent()) else {
            continue;
        };
        let inside_authored_visibility_range = (chunk.origin + transform.translation.xz())
            .distance(camera.focus)
            <= detail.solid_m + detail.fade_m;
        let root = children
            .into_iter()
            .flat_map(|children| children.iter())
            .find(|&child| prefab_roots.contains(child));
        match (inside_authored_visibility_range, root) {
            (true, None) => {
                let Some(prefab) = prefabs.get(&detail.prefab) else {
                    continue;
                };
                let (root, _, _) = spawn_prefab_render_tree(&mut commands, prefab, entity, false);
                commands.entity(root).insert(TerrainDetailRenderRoot);
            }
            (false, Some(root)) => {
                commands.entity(root).despawn();
            }
            _ => {}
        }
    }
}
