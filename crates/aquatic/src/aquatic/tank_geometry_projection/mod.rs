use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::topology::habitat_boundary_queries::HabitatBoundaryQueries;

use openzt2_zoo_state::aquatic::tank_wall_ground_sampling::TankWallGroundSampling;
use openzt2_zoo_state::habitat::habitat_types::HabitatChanged;
use openzt2_zoo_state::habitat::habitat_types::HabitatIndex;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::habitat::habitat_types::HabitatSummary;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged;
use openzt2_zoo_state::topology::topology_graph_types::{FenceSegment, TopologyGrid, TopologyNode};

use super::aquatic_simulation_types::{Tank, TankEnvironment, TankGeometry, TankWallTopHeight};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};

#[derive(Component, Default, PartialEq)]
pub(super) struct TankExcavationBoundary {
    pub(super) walls: Box<[[Vec2; 2]]>,
    pub(super) bounds: Rect,
    pub(super) fill_height_cm: i16,
}

#[derive(Message)]
pub(super) struct TankFloorTerrainDisturbed(pub(super) Entity);

#[derive(Component, PartialEq)]
pub(super) struct TankFloorLevelCm(pub(super) i16);

pub(super) fn sample_changed_tank_floor_levels(
    mut commands: Commands,
    mut tanks: Query<
        (Entity, &TankGeometry, Option<&mut TankFloorLevelCm>),
        (With<Tank>, Or<(Added<Tank>, Changed<TankGeometry>)>),
    >,
) {
    for (tank, geometry, current) in &mut tanks {
        let next = TankFloorLevelCm((geometry.floor_height * 100.0).round() as i16);
        if let Some(mut current) = current {
            current.set_if_neq(next);
        } else {
            commands.entity(tank).try_insert(next);
        }
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn project_changed_habitat_and_terrain_geometry_into_tanks(
    mut commands: Commands,
    mut habitat_changes: MessageReader<HabitatChanged>,
    mut terrain_changes: MessageReader<TerrainChanged>,
    chunks: Query<(&TerrainChunk, &WorldMember)>,
    changed_tanks: Query<Entity, (With<Tank>, Or<(Added<Tank>, Changed<HabitatRegion>)>)>,
    grids: Query<&TopologyGrid>,
    changed_grids: Query<(), Changed<TopologyGrid>>,
    habitats: Query<&HabitatIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    mut disturbed_floors: MessageWriter<TankFloorTerrainDisturbed>,
    mut tanks: Query<
        (
            Entity,
            &HabitatRegion,
            &WorldMember,
            &HabitatSummary,
            &mut TankGeometry,
            &mut TankEnvironment,
            Option<&mut TankExcavationBoundary>,
        ),
        With<Tank>,
    >,
    boundaries: HabitatBoundaryQueries,
    fences: Query<(FenceSegment, Option<&TankWallTopHeight>)>,
    changed_wall_tops: Query<Entity, Changed<TankWallTopHeight>>,
    (definitions, active_definitions): (Res<Assets<WorldDefinitionAsset>>, Res<WorldDefinitions>),
    (nodes, ground): (Query<&TopologyNode>, TankWallGroundSampling),
) {
    let surface = active_definitions
        .get(&definitions)
        .and_then(|view| view.tank_surface_policy());
    let mut habitat_changes: EntityHashSet = habitat_changes
        .read()
        .map(|change| change.habitat_entity)
        .chain(changed_tanks.iter())
        .collect();
    if terrain_assets.is_changed() || !changed_grids.is_empty() {
        habitat_changes.extend(tanks.iter().filter_map(|(tank, _, member, ..)| {
            (terrain_assets.is_changed() || changed_grids.contains(member.root)).then_some(tank)
        }));
    }
    let mut terrain_affected = EntityHashSet::default();
    let mut ground_affected = EntityHashSet::default();
    for change in terrain_changes.read() {
        let Ok((chunk, member)) = chunks.get(change.chunk) else {
            continue;
        };
        let (Ok(grid), Ok(habitats)) = (grids.get(member.root), habitats.get(member.root)) else {
            continue;
        };
        if !grid.spacing_m.is_finite() || grid.spacing_m <= 0.0 {
            continue;
        }
        let min = chunk.origin + change.min.as_vec2() * chunk.spacing_m;
        let max = chunk.origin + change.max.as_vec2() * chunk.spacing_m;
        let min = ((min - Vec2::splat(chunk.spacing_m) - grid.origin) / grid.spacing_m)
            .floor()
            .as_ivec2();
        let max = ((max + Vec2::splat(chunk.spacing_m) - grid.origin) / grid.spacing_m)
            .floor()
            .as_ivec2();
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                if let Some(tank) = habitats.habitat_for_topology_cell(IVec2::new(x, y)) {
                    terrain_affected.insert(tank);
                    if change.ground_height_changed {
                        ground_affected.insert(tank);
                    }
                }
            }
        }
    }
    let wall_affected: EntityHashSet = if surface.is_some() {
        changed_wall_tops
            .iter()
            .flat_map(|wall| boundaries.habitats(wall))
            .collect()
    } else {
        EntityHashSet::default()
    };
    let affected: EntityHashSet = habitat_changes
        .iter()
        .chain(&terrain_affected)
        .chain(&wall_affected)
        .copied()
        .collect();
    for tank in affected {
        let Ok((_, region, member, summary, mut geometry, mut environment, excavation)) =
            tanks.get_mut(tank)
        else {
            continue;
        };
        let changed_habitat = habitat_changes.contains(&tank);
        let terrain_changed = terrain_affected.contains(&tank);
        let wall_top_changed = wall_affected.contains(&tank);
        let ground_changed = ground_affected.contains(&tank);
        if ground_changed {
            disturbed_floors.write(TankFloorTerrainDisturbed(tank));
        }
        let mut next_geometry = *geometry;
        let mut next_environment = *environment;
        if let Some(surface) = surface.filter(|_| wall_top_changed) {
            let rim = boundaries
                .fences(tank)
                .filter_map(|wall| fences.get(wall).ok()?.1.map(|top| top.0))
                .fold(f32::INFINITY, f32::min);
            if rim.is_finite() {
                next_geometry.wall_height = rim;
                next_geometry.water_height = next_geometry
                    .water_height
                    .min(rim - surface.water_height_offset_cm as f32 / 100.0);
                next_geometry.volume = next_geometry.area * next_geometry.depth();
            }
        }
        if changed_habitat || terrain_changed {
            let lowest_ground = if changed_habitat || ground_changed || excavation.is_none() {
                grids.get(member.root).ok().and_then(|grid| {
                    let (lowest, next) = sample_tank_excavation_boundary(
                        member.root,
                        *grid,
                        boundaries.fences(tank),
                        &fences,
                        &nodes,
                        &ground,
                    )?;
                    if let Some(mut excavation) = excavation {
                        excavation.set_if_neq(next);
                    } else {
                        commands.entity(tank).try_insert(next);
                    }
                    Some(lowest)
                })
            } else {
                None
            };
            project_habitat_region_into_tank_geometry(
                region,
                summary,
                lowest_ground,
                &mut next_geometry,
                &mut next_environment,
            );
        }
        geometry.set_if_neq(next_geometry);
        environment.set_if_neq(next_environment);
    }
}

fn sample_tank_excavation_boundary(
    root: Entity,
    grid: TopologyGrid,
    boundary: impl Iterator<Item = Entity>,
    fences: &Query<(FenceSegment, Option<&TankWallTopHeight>)>,
    nodes: &Query<&TopologyNode>,
    ground: &TankWallGroundSampling,
) -> Option<(f32, TankExcavationBoundary)> {
    let mut lowest = f32::INFINITY;
    let mut height_sum = 0.0;
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    let walls = boundary
        .map(|fence| {
            let (edge, _) = fences.get(fence).ok()?;
            let a = nodes.get(edge.a()).ok()?.cell;
            let b = nodes.get(edge.b()).ok()?.cell;
            let a_height = ground.ground_height_at_topology_cell(root, a)?;
            let b_height = ground.ground_height_at_topology_cell(root, b)?;
            lowest = lowest.min(a_height).min(b_height);
            height_sum += a_height;
            let positions = [a, b].map(|cell| grid.cell_translation(cell).xz());
            min = min.min(positions[0]).min(positions[1]);
            max = max.max(positions[0]).max(positions[1]);
            Some(positions)
        })
        .collect::<Option<Box<[_]>>>()?;
    if walls.is_empty() {
        return None;
    }
    Some((
        lowest,
        TankExcavationBoundary {
            fill_height_cm: (height_sum / walls.len() as f32 * 100.0).round() as i16,
            walls,
            bounds: Rect::from_corners(min, max),
        },
    ))
}

fn project_habitat_region_into_tank_geometry(
    region: &HabitatRegion,
    summary: &HabitatSummary,
    wall_base_height: Option<f32>,
    geometry: &mut TankGeometry,
    environment: &mut TankEnvironment,
) {
    geometry.area = region.area_square_metres.max(0.0);
    geometry.volume = geometry.area * geometry.depth();
    if let Some(wall_base_height) = wall_base_height {
        geometry.wall_base_height = wall_base_height;
    }
    let total_area = summary.land_area_square_metres + summary.water_area_square_metres;
    environment.land_fraction_permille = if total_area > 0.0 {
        ((summary.land_area_square_metres / total_area) * 1000.0).clamp(0.0, 1000.0) as u16
    } else {
        0
    };
}
