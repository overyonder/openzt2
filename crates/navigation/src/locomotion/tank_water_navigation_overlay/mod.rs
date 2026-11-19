use bevy::{platform::collections::HashMap, prelude::*};

use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::aquatic::aquatic_simulation_types::{Tank, TankGeometry};
use openzt2_zoo_state::aquatic::tank_navigation_water::TankNavigationWater;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::locomotion::locomotion_types::ActiveTerrainDerivedNavigationGraph;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_navigation_change_types::{
    TerrainNavigationChanged, TerrainNavigationRestored,
};
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

use super::world_navigation_installation::TerrainDerivedNavigationGraphInstalled;

#[derive(Component, Default, PartialEq)]
pub(super) struct TankNavigationCells(Vec<(u32, u16)>);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn project_tank_water_onto_navigation_cells(
    mut commands: Commands,
    graph: Option<Res<ActiveTerrainDerivedNavigationGraph>>,
    grid: Res<TopologyGrid>,
    mut world: Query<
        (
            Entity,
            Ref<TerrainDerivedNavigationGraphInstalled>,
            &mut TankNavigationWater,
        ),
        With<WorldRoot>,
    >,
    mut tanks: Query<
        (
            Entity,
            Ref<TankGeometry>,
            Ref<HabitatRegion>,
            Option<&WorldMember>,
            Option<&mut TankNavigationCells>,
        ),
        With<Tank>,
    >,
    invalidated_tanks: Query<
        Entity,
        (
            With<TankNavigationCells>,
            Or<(Without<Tank>, Without<TankGeometry>, Without<HabitatRegion>)>,
        ),
    >,
    mut removed: RemovedComponents<TankNavigationCells>,
    chunks: Query<&TerrainChunk>,
    mut edits: MessageReader<TerrainNavigationChanged>,
    mut restores: MessageReader<TerrainNavigationRestored>,
) {
    let changed_bounds: Vec<Rect> = edits
        .read()
        .map(|edit| (edit.chunk, edit.min, edit.max))
        .chain(restores.read().map(|edit| (edit.chunk, edit.min, edit.max)))
        .filter_map(|(entity, min, max)| {
            let chunk = chunks.get(entity).ok()?;
            let min = chunk.origin + min.saturating_sub(UVec2::ONE).as_vec2() * chunk.spacing_m;
            let max = chunk.origin + (max.as_vec2() + Vec2::ONE) * chunk.spacing_m;
            Some(Rect::from_corners(min, max))
        })
        .collect();
    let removed = removed.read().count() != 0;
    let (Some(graph), Ok((world_entity, installation, mut water))) = (graph, world.single_mut())
    else {
        return;
    };
    let navigation = graph.terrain_derived_navigation_graph();
    let cell_size_m = f32::from(navigation.terrain_cell_size_centimetres) * 0.01;
    if cell_size_m <= 0.0 {
        return;
    }
    let count = (grid.spacing_m / cell_size_m).round().max(1.0) as i32;
    let reset = installation.is_changed() || grid.is_changed();
    let rebuild = reset || removed || !invalidated_tanks.is_empty();
    for entity in &invalidated_tanks {
        commands.entity(entity).remove::<TankNavigationCells>();
    }

    let mut pending_cells = bevy::ecs::entity::EntityHashMap::<TankNavigationCells>::default();
    let mut affected = HashMap::<u32, u16>::default();
    for (entity, geometry, region, member, mut current) in &mut tanks {
        if member.is_some_and(|member| member.root != world_entity) {
            continue;
        }
        if current.is_some()
            && !reset
            && !geometry.is_changed()
            && !region.is_changed()
            && !region.topology_cells.iter().any(|cell| {
                let origin = grid.cell_translation(cell.extend(0)).xz();
                let far = origin + Vec2::splat(count as f32 * cell_size_m);
                changed_bounds
                    .iter()
                    .any(|bounds| origin.cmple(bounds.max).all() && far.cmpge(bounds.min).all())
            })
        {
            continue;
        }
        let mut cells = Vec::new();
        for cell in &region.topology_cells {
            let origin = grid.cell_translation(cell.extend(0)).xz();
            for x in 0..count {
                for z in 0..count {
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
                        cells.push((index, u16::try_from(depth_cm).unwrap_or(u16::MAX)));
                    }
                }
            }
        }
        cells.sort_unstable_by_key(|&(cell, _)| cell);
        cells.dedup_by_key(|(cell, _)| *cell);
        if let Some(current) = current.as_mut() {
            if current.0 != cells {
                affected.extend(current.0.iter().map(|&(cell, _)| (cell, 0)));
                affected.extend(cells.iter().map(|&(cell, _)| (cell, 0)));
                **current = TankNavigationCells(cells);
            }
        } else {
            affected.extend(cells.iter().map(|&(cell, _)| (cell, 0)));
            pending_cells.insert(entity, TankNavigationCells(cells));
        }
    }
    let active_tanks = || {
        tanks
            .iter()
            .filter(|(_, _, _, member, _)| member.is_none_or(|member| member.root == world_entity))
            .filter_map(|(entity, _, _, _, cells)| cells.or_else(|| pending_cells.get(&entity)))
    };
    if rebuild {
        water.replace_cells(active_tanks().flat_map(|cells| cells.0.iter().copied()));
    } else if !affected.is_empty() {
        for cells in active_tanks() {
            for (&cell, depth) in &mut affected {
                if let Ok(index) = cells.0.binary_search_by_key(&cell, |&(key, _)| key) {
                    *depth = cells.0[index].1;
                }
            }
        }
        water.update_cells(affected);
    }
    for (tank, cells) in pending_cells {
        commands.entity(tank).insert(cells);
    }
}
