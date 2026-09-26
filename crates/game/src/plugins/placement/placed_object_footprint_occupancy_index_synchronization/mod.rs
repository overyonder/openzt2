//! Synchronizes authoritative placed-object footprints with the occupancy index.

use bevy::prelude::*;
use openzt2_game_data::world_definitions::object_placement::FootprintCellFlags;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::world_spawn::persistent_id_types::PersistentId;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    object_placement_definition_queries::{
        authored_object_entrance_definitions,
        project_authored_object_entrance_definition_to_component,
        resolve_object_placeable_definition, select_authored_footprint_for_eighth_turns,
    },
    object_placement_validation::{
        calculate_authored_object_placement_eighth_turns,
        calculate_object_placement_footprint_origin, rotate_object_placement_cell_by_quarter_turns,
    },
    PhysicsMovedPlacedObjectFootprint, PlacedObjectDefinitionReference,
    PlacedObjectFootprintOccupancy, PlacedObjectFootprintOccupancyIndex,
};

/// Reconstructs the sole occupancy accelerator after world spawning and persistence have hydrated
/// authoritative placement components. Loading may reserve and sort; settled
/// frames never run this system.
pub(super) fn rebuild_placed_object_footprint_occupancy_index(
    mut index: ResMut<PlacedObjectFootprintOccupancyIndex>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    placed: Query<(
        Entity,
        &PersistentId,
        Ref<PlacedObjectDefinitionReference>,
        Ref<PlacedObjectFootprintOccupancy>,
        Option<&PhysicsMovedPlacedObjectFootprint>,
    )>,
) {
    if !placed.iter().any(|(_, _, placeable, footprint, moving)| {
        placeable.is_added()
            || footprint.is_added()
            || match moving {
                None => footprint.is_changed(),
                Some(_) => false,
            }
    }) {
        return;
    }
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let mut ordered: Vec<_> = placed.iter().collect();
    ordered.sort_unstable_by_key(|(_, id, _, _, _)| id.0);
    let capacity = ordered
        .iter()
        .filter_map(|(_, _, placed, _, _)| {
            resolve_object_placeable_definition(catalogue, placed.definition)
        })
        .map(|definition| {
            definition
                .footprint
                .len()
                .max(definition.diagonal_footprint.len())
        })
        .sum();
    index.clear();
    index.reserve(capacity);
    for (entity, _, placeable, occupancy, _) in ordered {
        let Some(definition) = resolve_object_placeable_definition(catalogue, placeable.definition)
        else {
            continue;
        };
        select_authored_footprint_for_eighth_turns(definition, occupancy.eighth_turns)
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
    objects: Query<
        (Entity, &DefinitionId, &Transform, &WorldMember),
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
    for (entity, object, transform, member) in &objects {
        let Some(definition) = resolve_object_placeable_definition(catalogue, object.0) else {
            continue;
        };
        let Some((origin, eighth_turns)) = calculate_authored_object_placement_eighth_turns(
            definition, transform,
        )
        .and_then(|turns| {
            calculate_object_placement_footprint_origin(definition, transform, turns)
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
    mut moving: Query<
        (
            Entity,
            &PlacedObjectDefinitionReference,
            &Transform,
            &mut PlacedObjectFootprintOccupancy,
        ),
        (With<PhysicsMovedPlacedObjectFootprint>, Changed<Transform>),
    >,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, placeable, transform, mut occupancy) in &mut moving {
        let Some(definition) = resolve_object_placeable_definition(catalogue, placeable.definition)
        else {
            continue;
        };
        let Some((origin, eighth_turns)) = calculate_authored_object_placement_eighth_turns(
            definition, transform,
        )
        .and_then(|turns| {
            calculate_object_placement_footprint_origin(definition, transform, turns)
                .map(|origin| (origin, turns))
        }) else {
            continue;
        };
        if occupancy.origin == origin && occupancy.eighth_turns == eighth_turns {
            continue;
        }
        for cell in select_authored_footprint_for_eighth_turns(definition, occupancy.eighth_turns) {
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
        for cell in select_authored_footprint_for_eighth_turns(definition, eighth_turns) {
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
