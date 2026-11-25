use bevy::{platform::collections::HashMap, prelude::*};

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::aquatic::aquatic_simulation_types::{Tank, TankGeometry};
use openzt2_zoo_state::aquatic::tank_navigation_water::TankNavigationWater;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::locomotion::locomotion_types::ActiveTerrainDerivedNavigationGraph;
use openzt2_zoo_state::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_navigation_change_types::{
    TerrainNavigationChanged, TerrainNavigationRestored,
};
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

use super::world_navigation_installation::NavigationGraphReplaced;

#[derive(Component, Default, PartialEq)]
pub(super) struct TankNavigationCells {
    covered: Vec<u32>,
    wet: Vec<(u32, u16)>,
    bounds: Option<Rect>,
}

impl TankNavigationCells {
    fn refresh_edited_cells(
        mut current: Mut<Self>,
        navigation: &TerrainDerivedNavigationGraph,
        world_bounds: &[Rect],
        cell_size: f32,
        water_height: f32,
        affected: &mut HashMap<u32, u16>,
    ) {
        let Some(tank_bounds) = current.bounds else {
            return;
        };
        for edit in world_bounds {
            let min = edit.min.max(tank_bounds.min);
            let max = edit.max.min(tank_bounds.max);
            if !min.cmple(max).all() {
                continue;
            }
            let first = (Vec2::new(min.x, -max.y) / cell_size)
                .max(Vec2::ZERO)
                .floor()
                .as_uvec2();
            let last = (Vec2::new(max.x, -min.y) / cell_size).floor().as_uvec2();
            for y in first.y..=last.y {
                for x in first.x..=last.x {
                    let Some(cell) = navigation.calculate_navigation_node_for_terrain_cell([x, y])
                    else {
                        continue;
                    };
                    if current.covered.binary_search(&cell).is_err() {
                        continue;
                    }
                    let depth = water_depth(navigation, cell, water_height);
                    match current.wet.binary_search_by_key(&cell, |&(key, _)| key) {
                        Ok(slot) if depth == 0 => {
                            current.wet.remove(slot);
                        }
                        Ok(slot) if current.wet[slot].1 != depth => {
                            current.wet[slot].1 = depth;
                        }
                        Err(slot) if depth != 0 => {
                            current.wet.insert(slot, (cell, depth));
                        }
                        _ => continue,
                    }
                    affected.insert(cell, 0);
                }
            }
        }
    }
}

fn water_depth(navigation: &TerrainDerivedNavigationGraph, cell: u32, water_height: f32) -> u16 {
    navigation.find_navigation_node(cell).map_or(0, |node| {
        let depth = ((water_height * 100.0).round() as i32).saturating_sub(node.height_centimetres);
        u16::try_from(depth.max(0)).unwrap_or(u16::MAX)
    })
}

#[derive(Message)]
pub(super) struct TankNavigationWaterChanged(pub(super) Entity);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn project_tank_water_onto_navigation_cells(
    mut commands: Commands,
    mut installations: MessageReader<NavigationGraphReplaced>,
    mut world: Query<
        (
            Entity,
            &ActiveTerrainDerivedNavigationGraph,
            Ref<TopologyGrid>,
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
    phase: Res<State<GamePhase>>,
    mut water_changes: MessageWriter<TankNavigationWaterChanged>,
) {
    let changed_bounds: Vec<Rect> = edits
        .read()
        .map(|edit| (edit.chunk, edit.min, edit.max))
        .chain(restores.read().map(|edit| (edit.chunk, edit.min, edit.max)))
        .filter_map(|(entity, min, max)| {
            let chunk = chunks.get(entity).ok()?;
            Some(navigation_sample_edit_bounds(chunk, min, max))
        })
        .collect();
    let removed = removed.read().count() != 0;
    let graph_replaced = installations.read().count() != 0;
    let Ok((world_entity, graph, grid, mut water)) = world.single_mut() else {
        return;
    };
    let navigation = graph.terrain_derived_navigation_graph();
    let cell_size_m = f32::from(navigation.terrain_cell_size_centimetres) * 0.01;
    if cell_size_m <= 0.0 {
        return;
    }
    let count = (grid.spacing_m / cell_size_m).round().max(1.0) as i32;
    let reset = graph_replaced || grid.is_changed();
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
        if !reset && !geometry.is_changed() && !region.is_changed() {
            if let Some(current) = current.take() {
                if !changed_bounds.is_empty() {
                    TankNavigationCells::refresh_edited_cells(
                        current,
                        navigation,
                        &changed_bounds,
                        cell_size_m,
                        geometry.water_height,
                        &mut affected,
                    );
                }
                continue;
            }
        }
        let mut next = TankNavigationCells::default();
        for cell in &region.topology_cells {
            let origin = grid.cell_translation(cell.extend(0)).xz();
            let bounds =
                Rect::from_corners(origin, origin + Vec2::splat(count as f32 * cell_size_m));
            next.bounds = Some(
                next.bounds
                    .map_or(bounds, |previous| previous.union(bounds)),
            );
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
                    next.covered.push(index);
                }
            }
        }
        next.covered.sort_unstable();
        next.covered.dedup();
        next.wet
            .extend(next.covered.iter().copied().filter_map(|cell| {
                let depth = water_depth(navigation, cell, geometry.water_height);
                (depth != 0).then_some((cell, depth))
            }));
        if let Some(mut current) = current {
            if current.wet != next.wet {
                affected.extend(current.wet.iter().map(|&(cell, _)| (cell, 0)));
                affected.extend(next.wet.iter().map(|&(cell, _)| (cell, 0)));
            }
            current.set_if_neq(next);
        } else {
            affected.extend(next.wet.iter().map(|&(cell, _)| (cell, 0)));
            pending_cells.insert(entity, next);
        }
    }
    let active_tanks = || {
        tanks
            .iter()
            .filter(|(_, _, _, member, _)| member.is_none_or(|member| member.root == world_entity))
            .filter_map(|(entity, _, _, _, cells)| cells.or_else(|| pending_cells.get(&entity)))
    };
    let changed = if rebuild {
        water.replace_cells(active_tanks().flat_map(|cells| cells.wet.iter().copied()));
        true
    } else if !affected.is_empty() {
        for cells in active_tanks() {
            for (&cell, depth) in &mut affected {
                if let Ok(index) = cells.wet.binary_search_by_key(&cell, |&(key, _)| key) {
                    *depth = cells.wet[index].1;
                }
            }
        }
        affected.retain(|node, depth| water.depth_centimetres(*node).unwrap_or_default() != *depth);
        let changed = !affected.is_empty();
        if changed {
            water.update_cells(affected);
        }
        changed
    } else {
        false
    };
    if changed && *phase.get() == GamePhase::InGame {
        water_changes.write(TankNavigationWaterChanged(world_entity));
    }
    for (tank, cells) in pending_cells {
        commands.entity(tank).insert(cells);
    }
}
