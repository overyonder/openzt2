use bevy::{
    ecs::{entity::EntityHashSet, system::SystemParam},
    prelude::*,
    tasks::{ComputeTaskPool, TaskPool},
};

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::topology::topology_edit_types::TopologyChanged;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyNode;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use super::habitat_fence_inputs::{
    HabitatBoundaryRaster, HabitatFenceIncidences, HabitatFenceLattice,
};
use super::habitat_membership_and_containment::HabitatCellSectors;
use crate::habitat::habitat_region_derivation::{
    connected_fence_components, derive_enclosed_habitats,
};
use crate::habitat::habitat_terrain_summary_calculation::calculate_habitat_terrain_summary_from_topology_cells;
use crate::habitat::habitat_terrain_summary_refresh::derive_dominant_habitat_climate;
use crate::habitat::habitat_terrain_summary_refresh::read_terrain_fact_for_topology_cell;
use openzt2_zoo_state::habitat::habitat_cell_ownership_changes::HabitatCellOwnershipChanged;
use openzt2_zoo_state::habitat::habitat_topology_cell_queries::sorted_topology_cells_contain_cell;
use openzt2_zoo_state::habitat::habitat_types::HabitatChanged;
use openzt2_zoo_state::habitat::habitat_types::HabitatClimate;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::habitat::habitat_types::HabitatSummary;
use openzt2_zoo_state::habitat::habitat_types::RebuildHabitatRegionsWithinTopologyBounds;
use openzt2_zoo_state::habitat::habitat_types::{
    Habitat, HabitatBoundaryFence, HabitatCellOf, HabitatGridCell, HabitatIndex,
};

#[derive(Component)]
pub(super) struct HabitatSectorCell(usize);

pub(super) fn publish_animal_award_traversable_area(
    habitats: Query<(Ref<HabitatRegion>, Ref<HabitatSummary>)>,
    worlds: Query<&TopologyGrid, With<WorldRoot>>,
    mut animals: Query<
        (
            Entity,
            Ref<WorldMember>,
            Option<Ref<openzt2_zoo_state::habitat::habitat_types::HabitatMember>>,
            Option<&mut openzt2_progression::progression::AwardTraversableArea>,
        ),
        With<openzt2_zoo_state::animal_lifecycle::types::Animal>,
    >,
    mut commands: Commands,
) {
    use openzt2_progression::progression::AwardTraversableArea;
    for (entity, world, membership, area) in &mut animals {
        let Some((member, region, summary, grid)) = membership.as_ref().and_then(|member| {
            let (region, summary) = habitats.get(member.habitat_entity).ok()?;
            Some((member, region, summary, worlds.get(world.root).ok()?))
        }) else {
            if area.is_some() {
                commands.entity(entity).remove::<AwardTraversableArea>();
            }
            continue;
        };
        if area.is_some()
            && !world.is_changed()
            && !member.is_changed()
            && !region.is_changed()
            && !summary.is_changed()
        {
            continue;
        }
        let cell_area = grid.spacing_m * grid.spacing_m;
        if cell_area <= 0.0 {
            continue;
        }
        let next = AwardTraversableArea {
            total_cells: (region.area_square_metres / cell_area).round() as u32,
            biome_cells: summary
                .biome_areas_square_metres
                .iter()
                .map(|&(biome, area)| (biome, (area / cell_area).round() as u32))
                .collect(),
        };
        if let Some(mut area) = area {
            area.reborrow()
                .map_unchanged(|area| &mut area.total_cells)
                .set_if_neq(next.total_cells);
            area.map_unchanged(|area| &mut area.biome_cells)
                .set_if_neq(next.biome_cells);
        } else {
            commands.entity(entity).insert(next);
        }
    }
}

/// Seeds the first regional rebuild from topology already hydrated with the map.
pub(super) fn request_initial_habitat_region_rebuild_from_loaded_topology(
    topology_nodes: Query<&TopologyNode>,
    ready_worlds: Query<(), (With<WorldRoot>, With<TopologyGrid>, With<TerrainIndex>)>,
    mut habitat_rebuild_requests: MessageWriter<RebuildHabitatRegionsWithinTopologyBounds>,
) {
    if ready_worlds.is_empty() {
        return;
    }
    let Some(topology_bounds) = calculate_topology_cell_bounds(topology_nodes.iter()) else {
        return;
    };
    habitat_rebuild_requests.write(RebuildHabitatRegionsWithinTopologyBounds(topology_bounds));
}

pub(super) fn request_habitat_region_rebuild_for_added_topology_nodes(
    added_topology_nodes: Query<&TopologyNode, Added<TopologyNode>>,
    mut habitat_rebuild_requests: MessageWriter<RebuildHabitatRegionsWithinTopologyBounds>,
) {
    let Some(topology_bounds) = calculate_topology_cell_bounds(added_topology_nodes.iter()) else {
        return;
    };
    habitat_rebuild_requests.write(RebuildHabitatRegionsWithinTopologyBounds(topology_bounds));
}

pub(super) fn request_habitat_region_rebuilds_after_topology_changes(
    mut topology_changes: MessageReader<TopologyChanged>,
    mut habitat_rebuild_requests: MessageWriter<RebuildHabitatRegionsWithinTopologyBounds>,
) {
    for topology_change in topology_changes.read() {
        habitat_rebuild_requests.write(RebuildHabitatRegionsWithinTopologyBounds(
            topology_change.bounds,
        ));
    }
}

#[derive(SystemParam)]
pub(super) struct HabitatOwnershipQueries<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    cell_owners: Query<'w, 's, &'static HabitatCellOf>,
    sector_owners: Query<'w, 's, &'static mut HabitatCellSectors>,
    linked_sectors: Query<'w, 's, (&'static HabitatSectorCell, &'static HabitatCellOf)>,
    boundary_links: Query<'w, 's, &'static HabitatBoundaryFence>,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn rebuild_changed_habitat_regions_from_fence_topology_and_terrain(
    mut commands: Commands,
    mut habitat_rebuild_requests: MessageReader<RebuildHabitatRegionsWithinTopologyBounds>,

    terrain_assets: Res<Assets<TerrainAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    fence_incidences: HabitatFenceIncidences,
    rasters: Query<&HabitatBoundaryRaster>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    mut world_roots: Query<
        (
            Entity,
            &mut HabitatIndex,
            &TerrainIndex,
            &TopologyGrid,
            &HabitatFenceLattice,
        ),
        With<WorldRoot>,
    >,
    mut ownership: HabitatOwnershipQueries,
    mut existing_habitats: Query<
        (
            Entity,
            &mut HabitatRegion,
            &mut HabitatSummary,
            Option<&mut HabitatClimate>,
            Option<&WorldMember>,
        ),
        With<Habitat>,
    >,
    mut habitat_changes: MessageWriter<HabitatChanged>,
    mut cell_changes: MessageWriter<HabitatCellOwnershipChanged>,
) {
    let HabitatOwnershipQueries {
        children: habitat_children,
        cell_owners,
        sector_owners,
        linked_sectors,
        boundary_links,
    } = &mut ownership;
    let Some(changed_topology_bounds) =
        merge_requested_habitat_region_rebuild_bounds(&mut habitat_rebuild_requests)
    else {
        return;
    };
    debug!(
        ?changed_topology_bounds,
        roots = world_roots.iter().count(),
        "requested habitat rebuild"
    );
    let Ok((world_root, mut habitat_index, terrain_index, topology_grid, fence_lattice)) =
        world_roots.single_mut()
    else {
        return;
    };
    let topology_cell_size_metres = topology_grid.spacing_m;
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };

    let mut affected = EntityHashSet::default();
    let mut visited_fences = EntityHashSet::default();
    let mut loaded_fences = Vec::new();
    let mut bounds = changed_topology_bounds;
    // Expand using existing habitat interiors and connected fence footprints.
    // No fence geometry is derived while finding the affected components.
    loop {
        let before = bounds;
        let nearby = IRect::from_corners(bounds.min - IVec2::ONE, bounds.max + IVec2::ONE);
        for y in nearby.min.y..=nearby.max.y {
            for x in nearby.min.x..=nearby.max.x {
                let Some(cell) = habitat_index.cell_at(IVec2::new(x, y)) else {
                    continue;
                };
                let owners = sector_owners.get(cell).ok().map_or_else(
                    || {
                        [
                            cell_owners.get(cell).ok().map(|owner| owner.0),
                            None,
                            None,
                            None,
                        ]
                    },
                    |sectors| sectors.0,
                );
                for habitat in owners.into_iter().flatten() {
                    if affected.insert(habitat) {
                        if let Ok((_, region, ..)) = existing_habitats.get(habitat) {
                            if let Some(footprint) = calculate_topology_cell_bounds_of_cells(
                                region.topology_cells.iter().copied(),
                            ) {
                                bounds = bounds.union(IRect::from_corners(
                                    footprint.min,
                                    footprint.max + IVec2::ONE,
                                ));
                            }
                        }
                    }
                }
            }
        }
        let rectangle = bounds;
        let mut frontier = Vec::new();
        for y in rectangle.min.y..=rectangle.max.y {
            for x in rectangle.min.x..=rectangle.max.x {
                frontier.extend(
                    fence_incidences
                        .fences_at(fence_lattice, IVec2::new(x, y))
                        .filter(|fence| visited_fences.insert(*fence)),
                );
            }
        }
        while let Some(fence) = frontier.pop() {
            let Ok(raster) = rasters.get(fence) else {
                continue;
            };
            for piece in &raster.0 {
                for node in [piece.first_topology_node, piece.second_topology_node] {
                    bounds = bounds.union_point(node);
                    frontier.extend(
                        fence_incidences
                            .fences_at(fence_lattice, node)
                            .filter(|fence| visited_fences.insert(*fence)),
                    );
                }
                loaded_fences.push(*piece);
            }
        }
        if bounds == before {
            break;
        }
    }
    let components = connected_fence_components(&loaded_fences);
    // Nested or overlapping boundary components must share an outside flood.
    // Disjoint bounds can be solved independently without altering cell order.
    let mut groups: Vec<(IRect, Vec<_>)> = Vec::new();
    for component in components {
        let mut bounds = component.bounds;
        let mut segments = component.segments;
        let mut cursor = 0;
        while cursor < groups.len() {
            let (other, _) = &groups[cursor];
            if bounds.min.cmple(other.max).all() && bounds.max.cmpge(other.min).all() {
                let (other, moved) = groups.remove(cursor);
                bounds = bounds.union(other);
                segments.extend(moved);
                cursor = 0;
            } else {
                cursor += 1;
            }
        }
        groups.push((bounds, segments));
    }
    let mut derived_regions: Vec<_> = ComputeTaskPool::get_or_init(TaskPool::default)
        .scope(|scope| {
            for (_, segments) in groups {
                scope.spawn(async move { derive_enclosed_habitats(&segments) });
            }
        })
        .into_iter()
        .flatten()
        .collect();
    derived_regions
        .sort_unstable_by_key(|region| region.topology_cells.first().map(|cell| (cell.x, cell.y)));
    let mut affected_existing_habitats = affected
        .into_iter()
        .filter_map(|entity| {
            let (_, region, ..) = existing_habitats.get(entity).ok()?;
            Some((entity, region.topology_cells.clone()))
        })
        .collect::<Vec<_>>();
    affected_existing_habitats.sort_unstable_by_key(|(entity, _)| entity.to_bits());
    let mut reused_existing_habitats = vec![false; affected_existing_habitats.len()];
    let mut changed_sectors =
        bevy::platform::collections::HashMap::<IVec2, [Option<Entity>; 4]>::new();
    for (_, topology_cells) in &affected_existing_habitats {
        for &coordinate in topology_cells {
            changed_sectors.insert(coordinate, [None; 4]);
        }
    }

    for derived_region in derived_regions {
        let habitat_entity = find_best_reusable_habitat_region_index(
            &derived_region.topology_cells,
            &affected_existing_habitats,
            &reused_existing_habitats,
        )
        .map(|reusable_habitat_index| {
            reused_existing_habitats[reusable_habitat_index] = true;
            affected_existing_habitats[reusable_habitat_index].0
        })
        .unwrap_or_else(|| commands.spawn_empty().id());
        let topology_cell_area_square_metres =
            topology_cell_size_metres * topology_cell_size_metres;
        let mut read_terrain_cell_fact = |topology_cell| {
            read_terrain_fact_for_topology_cell(
                topology_cell,
                *topology_grid,
                &terrain_index,
                &terrain_assets,
                &terrain_chunks,
            )
        };
        let sector_cells: Vec<_> = derived_region
            .sectors
            .iter()
            .map(|&(cell, _)| cell)
            .collect();
        let habitat_summary = calculate_habitat_terrain_summary_from_topology_cells(
            &sector_cells,
            topology_cell_area_square_metres / 4.0,
            derived_region.boundary_is_breached,
            &mut read_terrain_cell_fact,
        );
        debug!(
            ?habitat_entity,
            ?habitat_summary,
            "derived habitat terrain summary"
        );
        let habitat_region = HabitatRegion {
            area_square_metres: topology_cell_area_square_metres
                * derived_region.sectors.len() as f32
                / 4.0,
            topology_cells: derived_region.topology_cells.into_boxed_slice(),
        };
        for (tile, side) in derived_region.sectors {
            changed_sectors.entry(tile).or_insert([None; 4])[usize::from(side / 2)] =
                Some(habitat_entity);
        }
        let habitat_climate = derive_dominant_habitat_climate(&habitat_summary, world_definitions);
        let member = WorldMember { root: world_root };
        if let Ok((_, mut region, mut summary, climate, world)) =
            existing_habitats.get_mut(habitat_entity)
        {
            region.set_if_neq(habitat_region);
            summary.set_if_neq(habitat_summary);
            match (climate, habitat_climate) {
                (Some(mut current), Some(next)) => {
                    current.set_if_neq(next);
                }
                (None, Some(next)) => {
                    commands.entity(habitat_entity).insert(next);
                }
                (Some(_), None) => {
                    commands.entity(habitat_entity).remove::<HabitatClimate>();
                }
                (None, None) => {}
            }
            if world != Some(&member) {
                commands.entity(habitat_entity).insert(member);
            }
        } else {
            let mut spawned = commands.entity(habitat_entity);
            spawned.insert((Habitat, habitat_region, habitat_summary, member));
            if let Some(climate) = habitat_climate {
                spawned.insert(climate);
            }
        }
        let existing_links = habitat_children
            .get(habitat_entity)
            .ok()
            .into_iter()
            .flat_map(|children| children.iter())
            .filter_map(|link| Some((link, boundary_links.get(link).ok()?.0)));
        let desired_fences = &derived_region.boundary_fence_entities;
        let shared_prefix = existing_links
            .clone()
            .zip(desired_fences)
            .take_while(|((_, existing), desired)| existing == *desired)
            .count();
        for (link, _) in existing_links.skip(shared_prefix) {
            commands.entity(link).try_despawn();
        }
        for &fence in &desired_fences[shared_prefix..] {
            commands.spawn((ChildOf(habitat_entity), HabitatBoundaryFence(fence)));
        }
        habitat_changes.write(HabitatChanged {
            habitat_entity,
            changed_topology_bounds,
        });
    }

    let mut changed_cells = Vec::new();
    let mut changed_sectors: Vec<_> = changed_sectors.into_iter().collect();
    changed_sectors.sort_unstable_by_key(|(tile, _)| (tile.x, tile.y));
    for (coordinate, desired) in changed_sectors {
        let mut centre_owner = desired[3];
        let uniform = desired.iter().all(|&owner| owner == centre_owner);
        let cell = match habitat_index.cell_at(coordinate) {
            Some(cell) => cell,
            None if desired.into_iter().any(|owner| owner.is_some()) => {
                let cell = commands
                    .spawn((HabitatGridCell(coordinate), ChildOf(world_root)))
                    .id();
                habitat_index.record_cell(coordinate, cell);
                cell
            }
            None => continue,
        };
        let mut proxies = [None; 4];
        if let Ok(children) = habitat_children.get(cell) {
            for child in children.iter() {
                let Ok((sector, owner)) = linked_sectors.get(child) else {
                    continue;
                };
                if !uniform && desired[sector.0] == Some(owner.0) {
                    proxies[sector.0] = Some(child);
                } else {
                    commands.entity(child).try_despawn();
                }
            }
        }
        if !uniform {
            for (sector, owner) in desired.into_iter().enumerate() {
                if let Some(owner) = owner.filter(|_| proxies[sector].is_none()) {
                    proxies[sector] = Some(
                        commands
                            .spawn((
                                ChildOf(cell),
                                HabitatSectorCell(sector),
                                HabitatCellOf(owner),
                            ))
                            .id(),
                    );
                }
            }
            let updated = HabitatCellSectors(desired);
            if let Ok(mut current) = sector_owners.get_mut(cell) {
                if current.set_if_neq(updated) {
                    changed_cells.push(coordinate);
                }
            } else {
                commands.entity(cell).insert(updated);
                changed_cells.push(coordinate);
            }
            habitat_index.set_cell_partition(coordinate, Some(proxies));
        } else if sector_owners.contains(cell) {
            commands.entity(cell).remove::<HabitatCellSectors>();
            habitat_index.set_cell_partition(coordinate, None);
            changed_cells.push(coordinate);
        }
        // Whole-cell consumers use the centre sector when querying a cut cell.
        let old = cell_owners.get(cell).ok().map(|current| current.0);
        if old != centre_owner {
            if let Some(owner) = centre_owner.take() {
                commands.entity(cell).insert(HabitatCellOf(owner));
            } else {
                commands.entity(cell).remove::<HabitatCellOf>();
            }
            changed_cells.push(coordinate);
        }
    }
    changed_cells.dedup();
    for (existing_habitat_index, (habitat_entity, _)) in
        affected_existing_habitats.iter().enumerate()
    {
        if !reused_existing_habitats[existing_habitat_index] {
            habitat_changes.write(HabitatChanged {
                habitat_entity: *habitat_entity,
                changed_topology_bounds,
            });
            commands.entity(*habitat_entity).despawn();
        }
    }
    if !changed_cells.is_empty() {
        cell_changes.write(HabitatCellOwnershipChanged {
            root: world_root,
            cells: changed_cells,
        });
    }
}

fn calculate_topology_cell_bounds<'a>(
    topology_nodes: impl Iterator<Item = &'a TopologyNode>,
) -> Option<IRect> {
    calculate_topology_cell_bounds_of_cells(
        topology_nodes.map(|topology_node| topology_node.cell.truncate()),
    )
}

fn calculate_topology_cell_bounds_of_cells(
    topology_cells: impl Iterator<Item = IVec2>,
) -> Option<IRect> {
    topology_cells.fold(None, |bounds: Option<IRect>, topology_cell| {
        Some(bounds.map_or_else(
            || IRect::from_corners(topology_cell, topology_cell),
            |bounds| {
                IRect::from_corners(bounds.min.min(topology_cell), bounds.max.max(topology_cell))
            },
        ))
    })
}

fn merge_requested_habitat_region_rebuild_bounds(
    habitat_rebuild_requests: &mut MessageReader<RebuildHabitatRegionsWithinTopologyBounds>,
) -> Option<IRect> {
    habitat_rebuild_requests
        .read()
        .fold(None, |merged_bounds, request| {
            Some(merged_bounds.map_or(request.0, |current: IRect| {
                IRect::from_corners(
                    current.min.min(request.0.min),
                    current.max.max(request.0.max),
                )
            }))
        })
}

fn find_best_reusable_habitat_region_index(
    derived_region_cells: &[IVec2],
    affected_existing_habitats: &[(Entity, Box<[IVec2]>)],
    reused_existing_habitats: &[bool],
) -> Option<usize> {
    affected_existing_habitats
        .iter()
        .enumerate()
        .filter(|(habitat_index, _)| !reused_existing_habitats[*habitat_index])
        .map(|(habitat_index, (_, existing_region_cells))| {
            let overlapping_cell_count = derived_region_cells
                .iter()
                .filter(|cell| sorted_topology_cells_contain_cell(existing_region_cells, **cell))
                .count();
            (habitat_index, overlapping_cell_count)
        })
        .filter(|(_, overlapping_cell_count)| *overlapping_cell_count > 0)
        .max_by_key(|(habitat_index, overlapping_cell_count)| {
            (*overlapping_cell_count, std::cmp::Reverse(*habitat_index))
        })
        .map(|(habitat_index, _)| habitat_index)
}
