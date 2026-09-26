use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::terrain::terrain_world_sampling::sample_terrain;
use crate::plugins::terrain::terrain_world_sampling::terrain_chunk_at;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_chunk_types::TerrainIndex;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::topology_graph_types::{PathSupport, PathTile, TopologyGrid, TopologyIndex};

/// Marks a path after its support has been resolved.
#[derive(Component)]
pub(super) struct PathSupportHydrated;

/// Prefab selected for a path support.
#[derive(Component)]
pub(super) struct PathSupportPrefab(pub(super) AssetId);

/// Adds a support beneath each elevated tile, anchored at the terrain height.
pub(super) fn hydrate_authored_path_supports_at_sampled_terrain_height(
    mut commands: Commands,
    paths: Query<(Entity, &PathTile, &WorldMember), Without<PathSupportHydrated>>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    terrain_index: Res<TerrainIndex>,
    topology_index: Res<TopologyIndex>,
    topology_grid: Res<TopologyGrid>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (path_entity, path, world_member) in &paths {
        let Some(definition) = definitions.find_path(path.definition) else {
            continue;
        };
        if !definition.elevated && path.cell.z <= 0 {
            commands.entity(path_entity).insert(PathSupportHydrated);
            continue;
        }
        let placement_step =
            super::ground_path_layout_calculation::calculate_snapped_ground_path_layout(
                path.cell,
                definition.width_cm,
                definitions.topology_cell_size_cm(),
            )
            .map_or(1, |(_, _, step)| step);
        let mut neighbours = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y]
            .into_iter()
            .filter(|offset| {
                topology_index
                    .paths
                    .contains_key(&(path.cell + *offset * placement_step))
            });
        let first_direction = neighbours.next();
        let path_turns = first_direction
            .zip(neighbours.next())
            .is_some_and(|(first, second)| first.dot(second) == 0);
        let curved_support_prefab = AssetId(definition.curve_support_prefab.0);
        let support_prefab = if path_turns && curved_support_prefab != AssetId::default() {
            curved_support_prefab
        } else {
            AssetId(definition.support_prefab.0)
        };
        let column = if path_turns {
            definition
                .curve_support_column
                .as_ref()
                .or(definition.support_column.as_ref())
        } else {
            definition.support_column.as_ref()
        };
        if support_prefab == AssetId::default() && column.is_none() {
            commands.entity(path_entity).insert(PathSupportHydrated);
            continue;
        }
        let path_world_position = topology_grid.cell_translation(path.cell);
        let path_world_horizontal_position = path_world_position.xz();
        let Some(ground_height_metres) =
            terrain_chunk_at(&terrain_index, path_world_horizontal_position)
                .and_then(|chunk_entity| terrain_chunks.get(chunk_entity).ok())
                .and_then(|(chunk, edited_samples)| {
                    let terrain_asset = terrain_assets.get(&chunk.asset)?;
                    sample_terrain(
                        chunk,
                        terrain_asset,
                        edited_samples,
                        path_world_horizontal_position,
                    )
                    .map(|sample| sample.height_m)
                })
        else {
            continue;
        };
        let support_entity = commands
            .spawn((
                PathSupport {
                    path: path_entity,
                    ground_height_m: ground_height_metres,
                },
                Transform::from_translation(Vec3::new(
                    0.0,
                    ground_height_metres - path_world_position.y,
                    0.0,
                )),
                ChildOf(path_entity),
                Visibility::Inherited,
                *world_member,
            ))
            .id();
        if let Some(column) = column {
            let direction = first_direction.unwrap_or(IVec3::Y);
            if !crate::plugins::world_spawn::expanding_column_presentation::spawn_expanding_column_presentation(
                &mut commands, definitions, support_entity, Transform::IDENTITY,
                path_world_position.y - ground_height_metres,
                Quat::from_rotation_y((direction.x as f32).atan2(direction.y as f32)), column,
            ) {
                commands.entity(support_entity).despawn();
                continue;
            }
        } else {
            commands
                .entity(support_entity)
                .insert(PathSupportPrefab(support_prefab));
        }
        commands.entity(path_entity).insert(PathSupportHydrated);
    }
}

pub(super) fn remove_path_supports_after_their_owning_path_is_removed(
    mut commands: Commands,
    supports: Query<(Entity, &PathSupport)>,
    paths: Query<(), With<PathTile>>,
    mut removed_paths: RemovedComponents<PathTile>,
) {
    for removed_path in removed_paths.read() {
        for (support_entity, support) in &supports {
            if support.path == removed_path && paths.get(support.path).is_err() {
                commands.entity(support_entity).despawn();
            }
        }
    }
}

pub(super) fn invalidate_path_supports_after_topology_or_terrain_changes(
    mut commands: Commands,
    mut topology_changes: MessageReader<super::topology_edit_types::TopologyChanged>,
    mut terrain_changes: MessageReader<crate::plugins::terrain::terrain_edit_types::TerrainChanged>,
    terrain_chunks: Query<&TerrainChunk>,
    paths: Query<(Entity, &PathTile), With<PathSupportHydrated>>,
    changed_paths: Query<Entity, (Changed<PathTile>, With<PathSupportHydrated>)>,
    supports: Query<(Entity, &PathSupport)>,
    grid: Res<TopologyGrid>,
) {
    let topology_changed = !topology_changes.is_empty();
    topology_changes.clear();
    let terrain_regions = terrain_changes
        .read()
        .filter_map(|change| {
            let chunk = terrain_chunks.get(change.chunk).ok()?;
            Some((
                chunk.origin + (change.min.as_vec2() - Vec2::ONE) * chunk.spacing_m,
                chunk.origin + (change.max.as_vec2() + Vec2::ONE) * chunk.spacing_m,
            ))
        })
        .collect::<Vec<_>>();
    if !topology_changed && terrain_regions.is_empty() && changed_paths.is_empty() {
        return;
    }
    let mut invalidated = std::collections::HashSet::new();
    for (entity, path) in &paths {
        let point = grid.cell_translation(path.cell).xz();
        if topology_changed
            || changed_paths.contains(entity)
            || terrain_regions
                .iter()
                .any(|(min, max)| point.cmpge(*min).all() && point.cmple(*max).all())
        {
            commands.entity(entity).remove::<PathSupportHydrated>();
            invalidated.insert(entity);
        }
    }
    for (entity, support) in &supports {
        if invalidated.contains(&support.path) {
            commands.entity(entity).despawn();
        }
    }
}
