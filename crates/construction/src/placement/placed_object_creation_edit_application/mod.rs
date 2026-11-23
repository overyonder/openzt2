//! Applies prepared placed-object creation edits.

use avian3d::prelude::RigidBody;
use bevy::gltf::Gltf;
use bevy::prelude::*;
use openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprints;

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_world::world_spawn::prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::habitat::habitat_types::HabitatLocatable;

use openzt2_game_data::world_definitions::object_placement::placement_stomp_rules::PlacementStompAction;
use openzt2_zoo_state::placement::object_placement_definition_queries::authored_object_entrance_definitions;
use openzt2_zoo_state::placement::object_placement_definition_queries::collect_occupied_placement_cells_for_transform;
use openzt2_zoo_state::placement::object_placement_definition_queries::project_authored_object_entrance_definition_to_component;
use openzt2_zoo_state::placement::object_placement_definition_queries::resolve_object_placeable_definition;
use openzt2_zoo_state::placement::object_placement_validation::calculate_authored_object_placement_eighth_turns;
use openzt2_zoo_state::placement::object_placement_validation::calculate_object_placement_footprint_origin;
use openzt2_zoo_state::placement::object_placement_validation::object_placement_stomp_action;
use openzt2_zoo_state::placement::placed_object_types::PhysicsMovedPlacedObjectFootprint;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectFootprintOccupancy;
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintQueries;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementCommitted;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementEditMutationKind;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementMutation;

pub(super) fn apply_prepared_placed_object_creation_edit(
    commands: &mut Commands,
    transaction: Entity,
    edit: &mut PreparedObjectPlacementMutation,
    catalogue: openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>,
    root: Entity,
    placement: &PlacedObjectFootprintQueries,
    scratch: &mut Vec<IVec2>,
    footprints: &ScenePlacementFootprints,
    prefabs: &Assets<ScenePrefabAsset>,
    models: &Assets<Gltf>,
    placed: &Query<(
        &PersistentId,
        &PlacedObjectDefinitionReference,
        &Transform,
        &PlacedObjectFootprintOccupancy,
    )>,
    stomped_occupants: &[Entity],
    repeat_placement: bool,
    tool: &mut ConstructionTool,
    object_placed: &mut MessageWriter<ObjectPlacementCommitted>,
) -> bool {
    let Some(definition) = resolve_object_placeable_definition(catalogue, edit.definition) else {
        return false;
    };
    let geometry = footprints.resolve(definition, Some(&edit.prefab));
    let exact_cells =
        collect_occupied_placement_cells_for_transform(geometry, &edit.transform, scratch)
            .is_some_and(|expected| expected == edit.cells.as_ref());
    if !exact_cells
        || edit.cells.iter().any(|cell| {
            placement
                .entities_occupying_cell(root, *cell)
                .filter(|occupant| !stomped_occupants.contains(occupant))
                .any(|entity| {
                    let occupant = placed
                        .get(entity)
                        .ok()
                        .map(|(_, placeable, _, _)| placeable.definition);
                    object_placement_stomp_action(definition, catalogue, occupant)
                        != PlacementStompAction::Allow
                })
        })
    {
        return false;
    }
    let Some(prefab) = prefabs.get(&edit.prefab) else {
        return false;
    };
    if first_missing_prefab_collider_model_asset_id(prefab, models).is_some() {
        return false;
    }
    let Some(turns) = calculate_authored_object_placement_eighth_turns(definition, &edit.transform)
    else {
        return false;
    };
    let Some(origin) =
        calculate_object_placement_footprint_origin(geometry, &edit.transform, turns)
    else {
        return false;
    };
    let spawned = spawn_loaded_scene_prefab_as_world_instance(
        commands,
        prefab,
        edit.prefab.clone(),
        root,
        edit.definition,
        edit.entity,
        edit.transform,
        true,
        None,
        if definition.moving_footprint {
            RigidBody::Dynamic
        } else {
            RigidBody::Static
        },
    );
    let authored_entrances = authored_object_entrance_definitions(definition);
    {
        let mut entity = commands.entity(spawned);
        entity.insert((
            PlacedObjectDefinitionReference {
                definition: edit.definition,
            },
            PlacedObjectFootprintOccupancy {
                origin,
                eighth_turns: turns,
            },
            openzt2_zoo_state::placement::placed_object_types::IndexedObjectPlacementCells(
                edit.cells.clone(),
            ),
            HabitatLocatable,
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
            WorldMember { root },
            ChildOf(spawned),
        ));
    }
    edit.applied_entity = Some(spawned);
    if edit.mutation == PreparedObjectPlacementEditMutationKind::Create {
        object_placed.write(ObjectPlacementCommitted {
            transaction,
            entity: spawned,
            definition: edit.definition,
        });
    }
    if !repeat_placement {
        if let Some(preview) = edit.preview {
            commands.entity(preview).despawn();
            *tool = ConstructionTool::Inspect;
        }
    }
    true
}
