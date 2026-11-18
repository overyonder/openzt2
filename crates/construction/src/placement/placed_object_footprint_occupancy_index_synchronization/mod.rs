//! Synchronizes authoritative placed-object footprints with the occupancy index.

use bevy::prelude::*;
use openzt2_game_data::world_definitions::object_placement::FootprintCellFlags;
use openzt2_world_state::world_spawn::prefab_source_asset_handle::PrefabSourceAssetHandle;
use openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprints;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_zoo_state::placement::object_placement_definition_queries::authored_object_entrance_definitions;
use openzt2_zoo_state::placement::object_placement_definition_queries::project_authored_object_entrance_definition_to_component;
use openzt2_zoo_state::placement::object_placement_definition_queries::resolve_object_placeable_definition;
use openzt2_zoo_state::placement::object_placement_validation::calculate_authored_object_placement_eighth_turns;
use openzt2_zoo_state::placement::object_placement_validation::calculate_object_placement_footprint_origin;
use openzt2_zoo_state::placement::object_placement_validation::rotate_object_placement_cell_by_quarter_turns;
use openzt2_zoo_state::placement::placed_object_types::PhysicsMovedPlacedObjectFootprint;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectFootprintOccupancy;
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex;

/// Reconstructs the sole occupancy accelerator after world spawning and persistence have hydrated
/// authoritative placement components. Loading may reserve and sort; settled
/// frames never run this system.
pub(super) fn rebuild_placed_object_footprint_occupancy_index(
    mut index: ResMut<PlacedObjectFootprintOccupancyIndex>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    footprints: Res<ScenePlacementFootprints>,
    mut placed: Query<(
        Entity,
        &PersistentId,
        Ref<PlacedObjectDefinitionReference>,
        &Transform,
        &mut PlacedObjectFootprintOccupancy,
        Option<&PhysicsMovedPlacedObjectFootprint>,
        Option<Ref<PrefabSourceAssetHandle>>,
    )>,
) {
    if !definitions.is_changed()
        && !active_definitions.is_changed()
        && !footprints.is_changed()
        && !placed
            .iter_mut()
            .any(|(_, _, placeable, _, footprint, moving, prefab)| {
                prefab.is_some_and(|prefab| prefab.is_changed())
                    || placeable.is_added()
                    || footprint.is_added()
                    || match moving {
                        None => footprint.is_changed(),
                        Some(_) => false,
                    }
            })
    {
        return;
    }
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let mut ordered: Vec<_> = placed.iter_mut().collect();
    ordered.sort_unstable_by_key(|(_, id, _, _, _, _, _)| id.0);
    index.clear();
    for (entity, _, placeable, transform, mut occupancy, _, prefab) in ordered {
        let Some(definition) = resolve_object_placeable_definition(catalogue, placeable.definition)
        else {
            continue;
        };
        let geometry = footprints.resolve(definition, prefab.as_ref().map(|prefab| &prefab.0));
        let Some(turns) = calculate_authored_object_placement_eighth_turns(definition, transform)
        else {
            continue;
        };
        let Some(origin) = calculate_object_placement_footprint_origin(geometry, transform, turns)
        else {
            continue;
        };
        // Late geometry changes both the derived pivot and the occupied cells.
        if occupancy.origin != origin || occupancy.eighth_turns != turns {
            occupancy.origin = origin;
            occupancy.eighth_turns = turns;
        }
        geometry
            .cells(occupancy.eighth_turns)
            .iter()
            .filter(|cell| cell.flags.contains_all(FootprintCellFlags::OCCUPIED))
            .for_each(|cell| {
                let local = IVec2::new(i32::from(cell.offset[0]), i32::from(cell.offset[1]));
                index.insert_entity_into_cell(
                    occupancy.origin
                        + rotate_object_placement_cell_by_quarter_turns(
                            local,
                            occupancy.eighth_turns / 2,
                        ),
                    entity,
                );
            });
    }
}

/// Projects loaded placement facts onto objects created by world hydration or
/// save restoration. Static footprint cells remain borrowed from world-definition catalog; the
/// entity stores only its definition, grid origin and authored orientation.
/// Docking points are ECS facts: the first remains on the placed entity for
/// the persistence contract and additional points are derived child entities.
pub(super) fn hydrate_placed_object_placement_components(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    footprints: Res<ScenePlacementFootprints>,
    objects: Query<
        (
            Entity,
            &DefinitionId,
            &Transform,
            &WorldMember,
            Option<&PrefabSourceAssetHandle>,
        ),
        (
            With<DefinitionId>,
            With<PersistentId>,
            Without<PlacedObjectDefinitionReference>,
        ),
    >,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, object, transform, member, prefab) in &objects {
        let Some(definition) = resolve_object_placeable_definition(catalogue, object.0) else {
            continue;
        };
        let geometry = footprints.resolve(definition, prefab.map(|prefab| &prefab.0));
        let Some((origin, eighth_turns)) = calculate_authored_object_placement_eighth_turns(
            definition, transform,
        )
        .and_then(|turns| {
            calculate_object_placement_footprint_origin(geometry, transform, turns)
                .map(|origin| (origin, turns))
        }) else {
            continue;
        };
        let authored_entrances = authored_object_entrance_definitions(definition);
        {
            let mut entity = commands.entity(entity);
            entity.insert((
                PlacedObjectDefinitionReference {
                    definition: object.0,
                },
                PlacedObjectFootprintOccupancy {
                    origin,
                    eighth_turns,
                },
            ));
            if let Some(entrance) = authored_entrances.first() {
                entity.insert(project_authored_object_entrance_definition_to_component(
                    entrance,
                ));
            }
            if definition.moving_footprint {
                entity.insert(PhysicsMovedPlacedObjectFootprint);
            }
        }
        for entrance in authored_entrances.iter().skip(1) {
            commands.spawn((
                project_authored_object_entrance_definition_to_component(entrance),
                *member,
                ChildOf(entity),
            ));
        }
    }
}

/// Applies Avian's completed transform writeback to placement's sole occupancy
/// accelerator. `PlacedObjectFootprintOccupancy` is the prior authoritative grid fact, so
/// changed objects remove only their old cells and insert their new cells.
pub(super) fn synchronize_physics_moved_object_footprints_with_occupancy_index(
    mut index: ResMut<PlacedObjectFootprintOccupancyIndex>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    footprints: Res<ScenePlacementFootprints>,
    mut moving: Query<
        (
            Entity,
            &PlacedObjectDefinitionReference,
            &Transform,
            &mut PlacedObjectFootprintOccupancy,
            Option<&PrefabSourceAssetHandle>,
        ),
        (With<PhysicsMovedPlacedObjectFootprint>, Changed<Transform>),
    >,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, placeable, transform, mut occupancy, prefab) in &mut moving {
        let Some(definition) = resolve_object_placeable_definition(catalogue, placeable.definition)
        else {
            continue;
        };
        let geometry = footprints.resolve(definition, prefab.map(|prefab| &prefab.0));
        let Some((origin, eighth_turns)) = calculate_authored_object_placement_eighth_turns(
            definition, transform,
        )
        .and_then(|turns| {
            calculate_object_placement_footprint_origin(geometry, transform, turns)
                .map(|origin| (origin, turns))
        }) else {
            continue;
        };
        if occupancy.origin == origin && occupancy.eighth_turns == eighth_turns {
            continue;
        }
        for cell in geometry.cells(occupancy.eighth_turns) {
            if !cell.flags.contains_all(FootprintCellFlags::OCCUPIED) {
                continue;
            }
            let local = IVec2::new(i32::from(cell.offset[0]), i32::from(cell.offset[1]));
            let old = occupancy.origin
                + rotate_object_placement_cell_by_quarter_turns(local, occupancy.eighth_turns / 2);
            index.remove_entity_from_cell(old, entity);
        }
        occupancy.origin = origin;
        occupancy.eighth_turns = eighth_turns;
        for cell in geometry.cells(eighth_turns) {
            if !cell.flags.contains_all(FootprintCellFlags::OCCUPIED) {
                continue;
            }
            let local = IVec2::new(i32::from(cell.offset[0]), i32::from(cell.offset[1]));
            index.insert_entity_into_cell(
                origin + rotate_object_placement_cell_by_quarter_turns(local, eighth_turns / 2),
                entity,
            );
        }
    }
}
