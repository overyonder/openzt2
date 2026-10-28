//! A tank's water is not terrain water. Navigation cells inside a tank whose
//! ground lies below that tank's water surface are water cells of the depth
//! between them, so swimmers route through them and walkers do not. Cells
//! that leave every tank return to their terrain values.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use openzt2_zoo_state::aquatic::aquatic_simulation_types::{Tank, TankGeometry};
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::locomotion::locomotion_types::{
    ActiveTerrainDerivedNavigationGraph, NavFlags,
};
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

/// Terrain values of the navigation cells a tank currently overrides.
#[derive(Default)]
pub(super) struct TankWaterNavigationCells {
    terrain_values: HashMap<u32, (bool, u16)>,
}

pub(super) fn project_tank_water_onto_navigation_cells(
    graph: Option<ResMut<ActiveTerrainDerivedNavigationGraph>>,
    grid: Res<TopologyGrid>,
    tanks: Query<(Ref<TankGeometry>, Ref<HabitatRegion>), With<Tank>>,
    mut removed_tanks: RemovedComponents<Tank>,
    mut overridden: Local<TankWaterNavigationCells>,
    mut tank_cells: Local<Vec<(u32, u16)>>,
) {
    let Some(mut graph) = graph else {
        return;
    };
    let tanks_removed = removed_tanks.read().count() > 0;
    let tanks_changed = tanks
        .iter()
        .any(|(geometry, region)| geometry.is_changed() || region.is_changed());
    if !graph.is_changed() && !tanks_changed && !tanks_removed {
        return;
    }
    let navigation = graph.terrain_derived_navigation_graph();
    let cell_size_m = f32::from(navigation.terrain_cell_size_centimetres) * 0.01;
    if cell_size_m <= 0.0 {
        return;
    }
    let terrain_cells_per_topology_cell = (grid.spacing_m / cell_size_m).round().max(1.0) as i32;
    tank_cells.clear();
    for (geometry, region) in &tanks {
        for cell in &region.topology_cells {
            let origin = grid.cell_translation(cell.extend(0)).xz();
            for x in 0..terrain_cells_per_topology_cell {
                for z in 0..terrain_cells_per_topology_cell {
                    let centre = origin + Vec2::new(x as f32 + 0.5, z as f32 + 0.5) * cell_size_m;
                    if centre.x < 0.0 || centre.y > 0.0 {
                        continue;
                    }
                    let terrain_cell = [
                        (centre.x / cell_size_m).floor() as u32,
                        (-centre.y / cell_size_m).floor() as u32,
                    ];
                    let Some(index) =
                        navigation.calculate_navigation_node_for_terrain_cell(terrain_cell)
                    else {
                        continue;
                    };
                    let Some(node) = navigation.find_navigation_node(index) else {
                        continue;
                    };
                    let depth_cm =
                        (geometry.water_height * 100.0).round() as i32 - node.height_centimetres;
                    if depth_cm > 0 {
                        tank_cells.push((index, u16::try_from(depth_cm).unwrap_or(u16::MAX)));
                    }
                }
            }
        }
    }
    let navigation = graph.terrain_derived_navigation_graph_mut();
    let still_tank_water = |index: &u32| tank_cells.iter().any(|(cell, _)| cell == index);
    overridden.terrain_values.retain(|index, (water, depth)| {
        if still_tank_water(index) {
            return true;
        }
        if let Some(node) = navigation.find_navigation_node_mut(*index) {
            node.eligibility_flags.0 = if *water {
                node.eligibility_flags.0 | NavFlags::WATER.0
            } else {
                node.eligibility_flags.0 & !NavFlags::WATER.0
            };
            node.water_depth_centimetres = *depth;
        }
        false
    });
    for &(index, depth_cm) in tank_cells.iter() {
        let Some(node) = navigation.find_navigation_node_mut(index) else {
            continue;
        };
        overridden.terrain_values.entry(index).or_insert((
            node.eligibility_flags.contains(NavFlags::WATER),
            node.water_depth_centimetres,
        ));
        node.eligibility_flags.0 |= NavFlags::WATER.0;
        node.water_depth_centimetres = depth_cm;
    }
}
