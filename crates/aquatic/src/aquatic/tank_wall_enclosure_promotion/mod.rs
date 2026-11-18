use bevy::prelude::*;
use bevy::{ecs::entity::EntityHashMap, platform::collections::HashSet};

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::{
    WorldDefinitions, WorldDefinitionsView,
};
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::{
    PendingTankRestoration, Tank, TankEnvironment, TankGeometry, TankWallTopHeight, TankWater, WaterQuality,
};
use openzt2_zoo_state::aquatic::tank_wall_ground_sampling::TankWallGroundSampling;
use openzt2_zoo_state::habitat::habitat_types::{HabitatIndex, HabitatRegion};
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::topology::topology_graph_types::{FenceEdge, TopologyNode};

use super::tank_geometry_rules::{
    new_tank_geometry, tank_geometry_limits, BoundaryGround, TankGeometryLimits,
};

const NEW_TANK_WATER_QUALITY: WaterQuality = WaterQuality(1_000);

#[derive(Component, PartialEq, Eq)]
pub(super) struct PendingTankPromotion {
    restore_next: bool,
}

fn mark_tank_promotion_pending(
    commands: &mut Commands,
    pending: &mut Query<(Entity, &mut PendingTankPromotion)>,
    habitat: Entity,
    restore_next: bool,
) {
    let next = PendingTankPromotion { restore_next };
    if let Ok((_, mut current)) = pending.get_mut(habitat) {
        current.set_if_neq(next);
    } else {
        commands.entity(habitat).insert(next);
    }
}

/// Ground and wall facts of a habitat whose whole boundary is tank wall.
struct TankWallBoundary {
    definition: AssetId,
    ground: BoundaryGround,
    lowest_wall_top: f32,
    missing_tops: Vec<(Entity, f32)>,
}

enum TankWallBoundaryReading {
    /// Some boundary edge is not a tank wall: the habitat is not a tank.
    NotTankWalls,
    /// Terrain or topology needed to read the walls is not available yet.
    NotReady,
    Walls(TankWallBoundary),
}

/// Reads a habitat boundary made only of tank wall segments. Any mix of tank
/// wall types closes one tank; its definition is that of the boundary edge
/// with the lowest persistent identifier. A wall without a recorded top gets
/// the rubber-band rule over its own two endpoints.
fn read_tank_wall_boundary(
    region: &HabitatRegion,
    definitions: WorldDefinitionsView<'_>,
    limits: TankGeometryLimits,
    fences: &Query<(&FenceEdge, &PersistentId, Option<&TankWallTopHeight>)>,
    nodes: &Query<&TopologyNode>,
    ground: &TankWallGroundSampling,
) -> TankWallBoundaryReading {
    if region.boundary_fence_entities.is_empty() {
        return TankWallBoundaryReading::NotTankWalls;
    }
    let mut definition = None;
    let mut missing_tops = Vec::new();
    let mut lowest_wall_top = f32::INFINITY;
    let mut sampled_cells = HashSet::new();
    let (mut lowest, mut sum) = (f32::INFINITY, 0.0);
    for fence in region.boundary_fence_entities.iter() {
        let Ok((edge, id, top)) = fences.get(*fence) else {
            return TankWallBoundaryReading::NotReady;
        };
        if definitions.find_fence(edge.definition).is_none() {
            return TankWallBoundaryReading::NotReady;
        }
        if definitions.find_tank(edge.definition).is_none() {
            return TankWallBoundaryReading::NotTankWalls;
        }
        if definition.is_none_or(|(lowest_id, _)| id.0 < lowest_id) {
            definition = Some((id.0, edge.definition));
        }
        let (Ok(a), Ok(b)) = (nodes.get(edge.a), nodes.get(edge.b)) else {
            return TankWallBoundaryReading::NotReady;
        };
        let mut endpoint_ground = [0.0; 2];
        for (index, cell) in [a.cell, b.cell].into_iter().enumerate() {
            let Some(height) = ground.ground_height_at_topology_cell(cell) else {
                return TankWallBoundaryReading::NotReady;
            };
            endpoint_ground[index] = height;
            if sampled_cells.insert(cell) {
                lowest = lowest.min(height);
                sum += height;
            }
        }
        let wall_top = top.map_or(
            limits.default_wall_height_m + endpoint_ground[0].max(endpoint_ground[1]),
            |top| top.0,
        );
        lowest_wall_top = lowest_wall_top.min(wall_top);
        if top.is_none() {
            missing_tops.push((*fence, wall_top));
        }
    }
    let Some((_, definition)) = definition else {
        return TankWallBoundaryReading::NotTankWalls;
    };
    TankWallBoundaryReading::Walls(TankWallBoundary {
        definition,
        ground: BoundaryGround {
            lowest,
            mean: sum / sampled_cells.len() as f32,
        },
        lowest_wall_top,
        missing_tops,
    })
}

/// Turns habitats closed entirely by tank walls into tanks, restoring saved
/// tank state when a pending restoration's seed cell lies in the habitat, even
/// one already promoted with new-tank defaults. Habitats that cannot be read
/// yet are retried on later ticks.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn promote_or_restore_habitats_enclosed_by_tank_walls(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    habitat_index: Res<HabitatIndex>,
    terrain_index: Res<TerrainIndex>,
    mut terrain_assets: MessageReader<AssetEvent<TerrainAsset>>,
    changed_habitats: Query<Entity, Changed<HabitatRegion>>,
    mut pending_habitats: Query<(Entity, &mut PendingTankPromotion)>,
    mut habitats: Query<(
        &HabitatRegion,
        &WorldMember,
        Option<&mut Tank>,
        Option<&mut TankGeometry>,
        Option<&mut TankWater>,
        Option<&mut TankEnvironment>,
        Option<&mut WaterQuality>,
    )>,
    pending_restorations: Query<(Entity, &PendingTankRestoration, &WorldMember)>,
    changed_restorations: Query<(), Changed<PendingTankRestoration>>,
    fences: Query<(&FenceEdge, &PersistentId, Option<&TankWallTopHeight>)>,
    nodes: Query<&TopologyNode>,
    changed_fences: Query<(), Changed<FenceEdge>>,
    changed_nodes: Query<(), Changed<TopologyNode>>,
    ground: TankWallGroundSampling,
    mut commands: Commands,
) {
    let terrain_loaded = terrain_assets.read().any(|event| {
        matches!(
            event,
            AssetEvent::Added { .. }
                | AssetEvent::Modified { .. }
                | AssetEvent::LoadedWithDependencies { .. }
        )
    });
    terrain_assets.clear();
    let inputs_changed = active_definitions.is_changed()
        || terrain_index.is_changed()
        || terrain_loaded
        || !changed_fences.is_empty()
        || !changed_nodes.is_empty();
    let mut candidates: Vec<_> = changed_habitats.iter().collect();
    candidates.extend(pending_habitats.iter().filter_map(|(habitat, pending)| {
        (inputs_changed || pending.restore_next).then_some(habitat)
    }));
    let resolve_changed_restorations =
        habitat_index.is_changed() || !changed_restorations.is_empty();
    if candidates.is_empty() && !resolve_changed_restorations {
        return;
    }
    let mut restorations: EntityHashMap<(Entity, &PendingTankRestoration, bool)> =
        EntityHashMap::default();
    for (entity, pending, world) in &pending_restorations {
        let Some(habitat) = habitat_index.habitat_for_topology_cell(pending.seed_cell) else {
            continue;
        };
        if habitats
            .get(habitat)
            .is_ok_and(|(_, member, ..)| member.root == world.root)
        {
            restorations
                .entry(habitat)
                .and_modify(|(_, _, multiple)| *multiple = true)
                .or_insert((entity, pending, false));
            if resolve_changed_restorations {
                candidates.push(habitat);
            }
        }
    }
    candidates.sort_unstable();
    candidates.dedup();
    let Some((definitions, limits)) = active_definitions
        .get(&definitions)
        .and_then(|definitions| Some((definitions, tank_geometry_limits(definitions)?)))
    else {
        for habitat in candidates {
            mark_tank_promotion_pending(&mut commands, &mut pending_habitats, habitat, false);
        }
        return;
    };
    for habitat in candidates {
        let Ok((region, member, tank, existing_geometry, water, environment, quality)) =
            habitats.get_mut(habitat)
        else {
            continue;
        };
        let walls =
            match read_tank_wall_boundary(region, definitions, limits, &fences, &nodes, &ground) {
                TankWallBoundaryReading::NotTankWalls => {
                    commands.entity(habitat).remove::<PendingTankPromotion>();
                    if tank.is_some() {
                        commands
                            .entity(habitat)
                            .remove::<(Tank, TankGeometry, TankWater, TankEnvironment)>();
                    }
                    continue;
                }
                TankWallBoundaryReading::NotReady => {
                    mark_tank_promotion_pending(
                        &mut commands,
                        &mut pending_habitats,
                        habitat,
                        false,
                    );
                    continue;
                }
                TankWallBoundaryReading::Walls(walls) => walls,
            };
        let restoration = restorations.get(&habitat).copied();
        let restored_wall_top = restoration
            .map(|(_, pending, _)| pending.geometry.wall_height)
            .or_else(|| {
                existing_geometry
                    .as_deref()
                    .map(|geometry| geometry.wall_height)
            });
        for (fence, top) in &walls.missing_tops {
            commands
                .entity(*fence)
                .insert_if_new(TankWallTopHeight(restored_wall_top.unwrap_or(*top)));
        }
        if let Some((pending_entity, pending, restore_next)) = restoration {
            let mut geometry = pending.geometry;
            if pending.water_limit_missing
                && geometry.water_height
                    < geometry.wall_height - limits.water_offset_m - f32::EPSILON
            {
                geometry.water_height_limit = geometry.water_height;
            }
            geometry.wall_base_height = walls.ground.lowest;
            geometry.area = region.area_square_metres.max(0.0);
            geometry.volume = geometry.area * geometry.depth();
            if !geometry.is_valid() {
                mark_tank_promotion_pending(&mut commands, &mut pending_habitats, habitat, false);
                continue;
            }
            commands.entity(pending_entity).despawn();
            if restore_next {
                mark_tank_promotion_pending(&mut commands, &mut pending_habitats, habitat, true);
            }
            let mut entity = commands.entity(habitat);
            if !restore_next {
                entity.remove::<PendingTankPromotion>();
            }
            let definition = Tank {
                definition: pending.definition,
            };
            if let Some(mut current) = tank {
                current.set_if_neq(definition);
            } else {
                entity.insert_if_new(definition);
            }
            if let Some(mut current) = existing_geometry {
                current.set_if_neq(geometry);
            } else {
                entity.insert_if_new(geometry);
            }
            if let Some(mut current) = water {
                current.set_if_neq(TankWater::default());
            } else {
                entity.insert_if_new(TankWater::default());
            }
            if let Some(mut current) = environment {
                current.set_if_neq(TankEnvironment::default());
            } else {
                entity.insert_if_new(TankEnvironment::default());
            }
            if let Some(mut current) = quality {
                current.set_if_neq(pending.water_quality);
            } else {
                entity.insert_if_new(pending.water_quality);
            }
            continue;
        }
        commands.entity(habitat).remove::<PendingTankPromotion>();
        if tank.is_some() {
            continue;
        }
        let Some(geometry) = new_tank_geometry(
            walls.lowest_wall_top,
            walls.ground,
            region.area_square_metres,
            limits,
        ) else {
            continue;
        };
        commands.entity(habitat).insert((
            Tank {
                definition: walls.definition,
            },
            geometry,
            TankWater::default(),
            TankEnvironment::default(),
            NEW_TANK_WATER_QUALITY,
        ));
    }
}
