use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;

use openzt2_zoo_state::aquatic::tank_wall_ground_sampling::TankWallGroundSampling;
use openzt2_zoo_state::habitat::habitat_types::HabitatChanged;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::habitat::habitat_types::HabitatSummary;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged;
use openzt2_zoo_state::topology::topology_graph_types::{FenceEdge, TopologyNode};

use super::aquatic_simulation_types::{Tank, TankEnvironment, TankGeometry, TankWallTopHeight};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};

#[allow(clippy::type_complexity)]
pub(super) fn project_changed_habitat_and_terrain_geometry_into_tanks(
    mut habitat_changes: MessageReader<HabitatChanged>,
    mut terrain_changes: MessageReader<TerrainChanged>,
    chunks: Query<&TerrainChunk>,
    added_tanks: Query<Entity, Added<Tank>>,
    mut tanks: Query<
        (
            Entity,
            &HabitatRegion,
            &HabitatSummary,
            &mut TankGeometry,
            &mut TankEnvironment,
        ),
        With<Tank>,
    >,
    fences: Query<(&FenceEdge, Option<&TankWallTopHeight>)>,
    changed_wall_tops: Query<Entity, Changed<TankWallTopHeight>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    nodes: Query<&TopologyNode>,
    ground: TankWallGroundSampling,
) {
    let surface = active_definitions
        .get(&definitions)
        .and_then(|view| view.tank_surface_policy());
    let changed_walls = changed_wall_tops.iter().collect::<EntityHashSet>();
    let changed_habitats = habitat_changes
        .read()
        .map(|change| change.habitat_entity)
        .chain(added_tanks.iter())
        .collect::<EntityHashSet>();
    let changed_terrain: Vec<_> = terrain_changes
        .read()
        .filter_map(|event| {
            let chunk = chunks.get(event.chunk).ok()?;
            let origin = chunk.coord * i32::from(chunk.side.saturating_sub(1));
            Some((
                origin + event.min.as_ivec2() - IVec2::ONE,
                origin + event.max.as_ivec2(),
                event.ground_height_changed,
            ))
        })
        .collect();
    if changed_walls.is_empty() && changed_habitats.is_empty() && changed_terrain.is_empty() {
        return;
    }
    for (tank, region, summary, mut geometry, mut environment) in &mut tanks {
        let changed_habitat = changed_habitats.contains(&tank);
        let mut terrain_changed = false;
        let mut ground_changed = false;
        for (minimum, maximum, height_changed) in &changed_terrain {
            if region
                .topology_cells
                .iter()
                .any(|cell| cell.cmpge(*minimum).all() && cell.cmple(*maximum).all())
            {
                terrain_changed = true;
                ground_changed |= *height_changed;
            }
        }
        let wall_top_changed = surface.is_some()
            && region
                .boundary_fence_entities
                .iter()
                .any(|wall| changed_walls.contains(wall));
        if !changed_habitat && !terrain_changed && !wall_top_changed {
            continue;
        }
        let mut next_geometry = *geometry;
        let mut next_environment = *environment;
        if let Some(surface) = surface.filter(|_| wall_top_changed) {
            let rim = region
                .boundary_fence_entities
                .iter()
                .filter_map(|wall| fences.get(*wall).ok()?.1.map(|top| top.0))
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
            let lowest_ground = if changed_habitat || ground_changed {
                lowest_ground_under_boundary(region, &fences, &nodes, &ground)
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

fn lowest_ground_under_boundary(
    region: &HabitatRegion,
    fences: &Query<(&FenceEdge, Option<&TankWallTopHeight>)>,
    nodes: &Query<&TopologyNode>,
    ground: &TankWallGroundSampling,
) -> Option<f32> {
    let mut lowest = None::<f32>;
    for fence in region.boundary_fence_entities.iter() {
        let (edge, _) = fences.get(*fence).ok()?;
        for node in [edge.a, edge.b] {
            let height = ground.ground_height_at_topology_cell(nodes.get(node).ok()?.cell)?;
            lowest = Some(lowest.map_or(height, |lowest| lowest.min(height)));
        }
    }
    lowest
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
