use bevy::prelude::*;

use crate::plugins::{
    habitat::habitat_types::{HabitatChanged, HabitatRegion, HabitatSummary},
    terrain::{terrain_chunk_types::TerrainChunk, terrain_edit_types::TerrainChanged},
};

use super::aquatic_simulation_types::{Tank, TankEnvironment, TankGeometry};

pub(super) fn project_changed_habitat_and_terrain_geometry_into_tanks(
    mut habitat_changes: MessageReader<HabitatChanged>,
    mut terrain_changes: MessageReader<TerrainChanged>,
    chunks: Query<&TerrainChunk>,
    mut tanks: Query<
        (
            &HabitatRegion,
            &HabitatSummary,
            &mut TankGeometry,
            &mut TankEnvironment,
        ),
        With<Tank>,
    >,
) {
    for event in habitat_changes.read() {
        let Ok((region, summary, mut geometry, mut environment)) =
            tanks.get_mut(event.habitat_entity)
        else {
            continue;
        };
        project_habitat_region_into_tank_geometry(region, summary, &mut geometry, &mut environment);
    }

    for event in terrain_changes.read() {
        let Ok(chunk) = chunks.get(event.chunk) else {
            continue;
        };
        let origin = chunk.coord * i32::from(chunk.side.saturating_sub(1));
        let minimum_changed_cell = origin + event.min.as_ivec2();
        let maximum_changed_cell = origin + event.max.as_ivec2();
        for (region, summary, mut geometry, mut environment) in &mut tanks {
            if !region.topology_cells.iter().any(|cell| {
                cell.cmpge(minimum_changed_cell).all() && cell.cmple(maximum_changed_cell).all()
            }) {
                continue;
            }
            project_habitat_region_into_tank_geometry(
                region,
                summary,
                &mut geometry,
                &mut environment,
            );
        }
    }
}

fn project_habitat_region_into_tank_geometry(
    region: &HabitatRegion,
    summary: &HabitatSummary,
    geometry: &mut TankGeometry,
    environment: &mut TankEnvironment,
) {
    geometry.area = region.area_square_metres.max(0.0);
    geometry.volume = geometry.area * geometry.depth();
    let total_area = summary.land_area_square_metres + summary.water_area_square_metres;
    environment.land_fraction_permille = if total_area > 0.0 {
        ((summary.land_area_square_metres / total_area) * 1000.0).clamp(0.0, 1000.0) as u16
    } else {
        0
    };
}
