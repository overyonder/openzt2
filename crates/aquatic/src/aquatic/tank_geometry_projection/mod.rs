use avian3d::collision::contact_types::ContactGraph;
use bevy::ecs::entity::EntityHashMap;
use bevy::ecs::{entity::EntityHashSet, entity_disabling::Disabled, query::Allow};
use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_navigation::locomotion::authored_tank_gate_exit::{
    RegisteredTankGate, TankLinkedPhysicalObject,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentId, world_membership_types::DefinitionId,
};
use openzt2_zoo_state::topology::habitat_boundary_queries::HabitatBoundaryQueries;
use openzt2_zoo_state::{
    habitat::habitat_membership_and_containment::locate_habitat_at_world_position,
    locomotion::locomotion_types::NavAgent,
    topology::gate_operation_types::{AuthoredGateVariant, Gate},
};

use openzt2_zoo_state::aquatic::tank_wall_ground_sampling::TankWallGroundSampling;
use openzt2_zoo_state::habitat::habitat_membership_and_containment::locate_habitat_at_topology_cell;
use openzt2_zoo_state::habitat::habitat_types::HabitatChanged;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::habitat::habitat_types::HabitatSummary;
use openzt2_zoo_state::habitat::habitat_types::{HabitatCellOf, HabitatIndex};
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged;
use openzt2_zoo_state::topology::topology_graph_types::{FenceSegment, TopologyGrid, TopologyNode};

use super::aquatic_simulation_types::{
    Tank, TankEnvironment, TankGeometry, TankWallTopHeight, TankWaterCellsChanged,
};
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

#[derive(Component, PartialEq)]
pub(super) struct TankWaterContactFootprint {
    root: Entity,
    cells: Box<[IVec2]>,
}

pub(super) fn publish_changed_tank_water_cells(
    mut commands: Commands,
    mut changes: MessageWriter<TankWaterCellsChanged>,
    mut tanks: Query<
        (
            Entity,
            Ref<WorldMember>,
            Ref<TankGeometry>,
            Ref<HabitatRegion>,
            Option<&mut TankWaterContactFootprint>,
        ),
        With<Tank>,
    >,
) {
    for (tank, member, geometry, region, previous) in &mut tanks {
        if previous.is_some()
            && !member.is_changed()
            && !geometry.is_changed()
            && !region.is_changed()
        {
            continue;
        }
        let cells: Box<[_]> = if geometry.is_valid() && geometry.depth() > 0.0 {
            region.topology_cells.clone()
        } else {
            Box::new([])
        };
        let mut affected = cells.to_vec();
        if let Some(previous) = previous.as_ref() {
            if previous.root == member.root {
                affected.extend_from_slice(&previous.cells);
            } else if !previous.cells.is_empty() {
                changes.write(TankWaterCellsChanged {
                    root: previous.root,
                    tank,
                    cells: previous.cells.clone(),
                });
            }
        }
        if !affected.is_empty() {
            affected.sort_unstable_by_key(|cell| (cell.y, cell.x));
            affected.dedup();
            changes.write(TankWaterCellsChanged {
                root: member.root,
                tank,
                cells: affected.into_boxed_slice(),
            });
        }
        let next = TankWaterContactFootprint {
            root: member.root,
            cells,
        };
        if let Some(mut previous) = previous {
            previous.set_if_neq(next);
        } else {
            commands.entity(tank).insert(next);
        }
    }
}

pub(super) fn retire_tank_water_contact_footprint(
    removed: On<Remove<Tank>>,
    mut commands: Commands,
) {
    commands
        .entity(removed.entity)
        .try_remove::<TankWaterContactFootprint>();
}

pub(super) fn publish_retired_tank_water_cells(
    removed: On<Discard<TankWaterContactFootprint>>,
    footprints: Query<&TankWaterContactFootprint, Allow<Disabled>>,
    mut changes: MessageWriter<TankWaterCellsChanged>,
) {
    let Ok(footprint) = footprints.get(removed.entity) else {
        return;
    };
    if !footprint.cells.is_empty() {
        changes.write(TankWaterCellsChanged {
            root: footprint.root,
            tank: removed.entity,
            cells: footprint.cells.clone(),
        });
    }
}

pub(super) fn publish_registered_tank_gates(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut gates: Query<
        (
            Entity,
            &WorldMember,
            Option<FenceSegment>,
            Option<&PersistentId>,
            Option<&AuthoredGateVariant>,
            Option<&mut RegisteredTankGate>,
            Has<Gate>,
        ),
        Or<(With<Gate>, With<RegisteredTankGate>)>,
    >,
    roots: Query<(&TopologyGrid, &HabitatIndex)>,
    cell_owners: Query<&HabitatCellOf>,
    nodes: Query<&TopologyNode>,
    tanks: Query<&WorldMember, With<Tank>>,
    boundaries: HabitatBoundaryQueries,
    children: Query<&Children>,
    platforms: Query<(&DefinitionId, &GlobalTransform)>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let gate_class = AssetId::from_key("tankgate");
    let platform_class = AssetId::from_key("tankplatform");
    for (gate, world, segment, persistent, variant, current, is_gate) in &mut gates {
        let next = (|| {
            let segment = segment.filter(|_| is_gate)?;
            let order = persistent?.0;
            let gate_definition = variant
                .and_then(|variant| catalogue.find_fence(variant.0))
                .or_else(|| catalogue.find_gate_fence(segment.definition));
            if !gate_definition
                .into_iter()
                .chain(catalogue.find_fence(segment.definition))
                .filter_map(|definition| catalogue.find_object(definition.object))
                .any(|object| object.behavior_type_identifiers.contains(&gate_class))
            {
                return None;
            }
            let (grid, habitats) = roots.get(world.root).ok()?;
            let a = grid
                .cell_translation(nodes.get(segment.a()).ok()?.cell)
                .xz();
            let b = grid
                .cell_translation(nodes.get(segment.b()).ok()?.cell)
                .xz();
            let direction = (b - a).try_normalize()?;
            let normal = Vec2::new(-direction.y, direction.x);
            let midpoint = (a + b) * 0.5;
            let owns_gate = |tank| {
                tanks
                    .get(tank)
                    .is_ok_and(|tank_world| tank_world.root == world.root)
                    && boundaries.habitats(gate).any(|habitat| habitat == tank)
            };
            let tank = [normal, -normal]
                .into_iter()
                .filter_map(|side| {
                    locate_habitat_at_world_position(
                        habitats,
                        &cell_owners,
                        midpoint + side * grid.spacing_m * 0.5,
                        *grid,
                    )
                })
                .find(|&tank| owns_gate(tank))?;
            let mut directed_platforms: [Option<(Entity, f32)>; 2] = [None, None];
            for platform in children.iter_descendants_depth_first::<Children>(gate) {
                let Ok((definition, pose)) = platforms.get(platform) else {
                    continue;
                };
                if !catalogue.find_object(definition.0).is_some_and(|object| {
                    object.behavior_type_identifiers.contains(&platform_class)
                }) {
                    continue;
                }
                let offset = pose.translation().xz() - midpoint;
                let signed_distance = offset.dot(normal);
                if !signed_distance.is_finite() || signed_distance == 0.0 {
                    continue;
                }
                let side = usize::from(signed_distance < 0.0);
                let distance = offset.length_squared();
                if directed_platforms[side].is_none_or(|(_, prior)| distance < prior) {
                    directed_platforms[side] = Some((platform, distance));
                }
            }
            let platforms = directed_platforms.map(|platform| platform.map(|(entity, _)| entity));
            Some(RegisteredTankGate {
                registration_order: order,
                tank,
                physical_sides: platforms,
                platforms,
                door: Some(gate),
            })
        })();
        match (current, next) {
            (Some(mut current), Some(next))
                if current.registration_order != next.registration_order
                    || current.tank != next.tank
                    || current.physical_sides != next.physical_sides
                    || current.platforms != next.platforms
                    || current.door != next.door =>
            {
                *current = next;
            }
            (None, Some(next)) => {
                commands.entity(gate).insert(next);
            }
            (Some(_), None) => {
                commands.entity(gate).remove::<RegisteredTankGate>();
            }
            _ => {}
        }
    }
}

pub(super) fn publish_tank_linked_physical_objects(
    mut commands: Commands,
    contacts: Option<Res<ContactGraph>>,
    gates: Query<(&RegisteredTankGate, &WorldMember)>,
    platforms: Query<Entity>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    actors: Query<&WorldMember, With<NavAgent>>,
    mut current_links: Query<(Entity, &mut TankLinkedPhysicalObject)>,
) {
    let mut linked = EntityHashMap::<(u64, Entity)>::default();
    if let Some(contacts) = contacts {
        for (gate, world) in &gates {
            for platform in gate.physical_sides.iter().flatten().copied() {
                if !platforms.contains(platform) {
                    continue;
                }
                for collider in std::iter::once(platform)
                    .chain(children.iter_descendants_depth_first::<Children>(platform))
                {
                    for pair in contacts
                        .contact_pairs_with(collider)
                        .filter(|pair| pair.is_touching())
                    {
                        let other = if pair.collider1 == collider {
                            pair.body2.unwrap_or(pair.collider2)
                        } else {
                            pair.body1.unwrap_or(pair.collider1)
                        };
                        let Some(actor) = std::iter::once(other)
                            .chain(parents.iter_ancestors::<ChildOf>(other))
                            .find(|&entity| {
                                actors
                                    .get(entity)
                                    .is_ok_and(|member| member.root == world.root)
                            })
                        else {
                            continue;
                        };
                        let candidate = (gate.registration_order, platform);
                        let selected = linked.entry(actor).or_insert(candidate);
                        if candidate.0 < selected.0 {
                            *selected = candidate;
                        }
                    }
                }
            }
        }
    }
    for (actor, mut current) in &mut current_links {
        if let Some((_, platform)) = linked.remove(&actor) {
            if current.0 != platform {
                current.0 = platform;
            }
        } else {
            commands.entity(actor).remove::<TankLinkedPhysicalObject>();
        }
    }
    for (actor, (_, platform)) in linked {
        commands
            .entity(actor)
            .insert(TankLinkedPhysicalObject(platform));
    }
}

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
    (habitats, cell_owners): (Query<&HabitatIndex>, Query<&HabitatCellOf>),
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
                if let Some(tank) =
                    locate_habitat_at_topology_cell(habitats, &cell_owners, IVec2::new(x, y))
                {
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
