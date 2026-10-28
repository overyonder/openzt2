use bevy::platform::collections::HashSet;
use bevy::prelude::*;

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
use openzt2_zoo_state::topology::topology_graph_types::{FenceEdge, TopologyNode};

use super::tank_geometry_rules::{
    new_tank_geometry, tank_geometry_limits, BoundaryGround, TankGeometryLimits,
};

const NEW_TANK_WATER_QUALITY: WaterQuality = WaterQuality(1_000);

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
#[allow(clippy::too_many_arguments)]
pub(super) fn promote_or_restore_habitats_enclosed_by_tank_walls(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    habitat_index: Res<HabitatIndex>,
    changed_habitats: Query<Entity, Changed<HabitatRegion>>,
    habitats: Query<(
        &HabitatRegion,
        Has<Tank>,
        &WorldMember,
        Option<&TankGeometry>,
    )>,
    pending_restorations: Query<(Entity, &PendingTankRestoration, &WorldMember)>,
    fences: Query<(&FenceEdge, &PersistentId, Option<&TankWallTopHeight>)>,
    nodes: Query<&TopologyNode>,
    ground: TankWallGroundSampling,
    mut deferred_habitats: Local<Vec<Entity>>,
    mut commands: Commands,
) {
    let mut candidates = std::mem::take(&mut *deferred_habitats);
    candidates.extend(changed_habitats.iter());
    candidates.extend(
        pending_restorations.iter().filter_map(|(_, pending, _)| {
            habitat_index.habitat_for_topology_cell(pending.seed_cell)
        }),
    );
    candidates.sort_unstable();
    candidates.dedup();
    let Some((definitions, limits)) = active_definitions
        .get(&definitions)
        .and_then(|definitions| Some((definitions, tank_geometry_limits(definitions)?)))
    else {
        deferred_habitats.extend(
            candidates
                .iter()
                .filter(|habitat| habitats.get(**habitat).is_ok()),
        );
        return;
    };
    for habitat in candidates {
        let Ok((region, is_tank, member, existing_geometry)) = habitats.get(habitat) else {
            continue;
        };
        let walls =
            match read_tank_wall_boundary(region, definitions, limits, &fences, &nodes, &ground) {
                TankWallBoundaryReading::NotTankWalls => {
                    if is_tank {
                        commands
                            .entity(habitat)
                            .remove::<(Tank, TankGeometry, TankWater, TankEnvironment)>();
                    }
                    continue;
                }
                TankWallBoundaryReading::NotReady => {
                    deferred_habitats.push(habitat);
                    continue;
                }
                TankWallBoundaryReading::Walls(walls) => walls,
            };
        let restoration = pending_restorations
            .iter()
            .find(|(_, pending, pending_member)| {
                pending_member.root == member.root
                    && region
                        .topology_cells
                        .binary_search_by_key(&(pending.seed_cell.x, pending.seed_cell.y), |cell| {
                            (cell.x, cell.y)
                        })
                        .is_ok()
            });
        let restored_wall_top = restoration
            .map(|(_, pending, _)| pending.geometry.wall_height)
            .or_else(|| existing_geometry.map(|geometry| geometry.wall_height));
        for (fence, top) in &walls.missing_tops {
            commands
                .entity(*fence)
                .insert(TankWallTopHeight(restored_wall_top.unwrap_or(*top)));
        }
        if let Some((pending_entity, pending, _)) = restoration {
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
                continue;
            }
            commands.entity(pending_entity).despawn();
            commands.entity(habitat).insert((
                Tank {
                    definition: pending.definition,
                },
                geometry,
                TankWater::default(),
                TankEnvironment::default(),
                pending.water_quality,
            ));
            continue;
        }
        if is_tank {
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
