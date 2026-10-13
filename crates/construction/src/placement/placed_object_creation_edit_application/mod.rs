//! Applies prepared placed-object creation edits.

use avian3d::prelude::RigidBody;
use bevy::gltf::Gltf;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_world::world_spawn::prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::habitat::habitat_types::HabitatLocatable;

use openzt2_zoo_state::placement::object_placement_definition_queries::authored_object_entrance_definitions;
use openzt2_zoo_state::placement::object_placement_definition_queries::collect_occupied_placement_cells_for_transform;
use openzt2_zoo_state::placement::object_placement_definition_queries::project_authored_object_entrance_definition_to_component;
use openzt2_zoo_state::placement::object_placement_definition_queries::resolve_object_placeable_definition;
use openzt2_zoo_state::placement::object_placement_validation::calculate_authored_object_placement_eighth_turns;
use openzt2_zoo_state::placement::object_placement_validation::calculate_object_placement_footprint_origin;
use openzt2_zoo_state::placement::object_placement_validation::object_placement_cell_is_blocked_by_existing_object;
use openzt2_zoo_state::placement::placed_object_types::PhysicsMovedPlacedObjectFootprint;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectFootprintOccupancy;
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementCellCollectionScratch;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementCommitted;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementEditMutationKind;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementMutation;

pub(super) fn apply_prepared_placed_object_creation_edit(
    commands: &mut Commands,
    transaction: Entity,
    edit: &mut PreparedObjectPlacementMutation,
    catalogue: openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>,
    root: Option<Entity>,
    placement: &mut PlacedObjectFootprintOccupancyIndex,
    scratch: &mut ObjectPlacementCellCollectionScratch,
    prefabs: &Assets<ScenePrefabAsset>,
    models: &Assets<Gltf>,
    placed: &Query<(
        &PersistentId,
        &PlacedObjectDefinitionReference,
        &mut Transform,
        &mut PlacedObjectFootprintOccupancy,
    )>,
    pending_created_definitions: &HashMap<Entity, AssetId>,
    repeat_placement: bool,
    tool: &mut ConstructionTool,
    object_placed: &mut MessageWriter<ObjectPlacementCommitted>,
) -> bool {
    let Some(definition) = resolve_object_placeable_definition(catalogue, edit.definition) else {
        return false;
    };
    let exact_cells = collect_occupied_placement_cells_for_transform(
        definition,
        &edit.transform,
        &mut scratch.cells,
    )
    .is_some_and(|expected| expected == edit.cells.as_ref());
    let Some(root) = root else {
        return false;
    };
    if !exact_cells
        || edit.cells.iter().any(|cell| {
            object_placement_cell_is_blocked_by_existing_object(
                placement,
                *cell,
                None,
                definition,
                catalogue,
                |entity| {
                    placed
                        .get(entity)
                        .ok()
                        .map(|(_, placeable, _, _)| placeable.definition)
                        .or_else(|| pending_created_definitions.get(&entity).copied())
                },
            )
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
        calculate_object_placement_footprint_origin(definition, &edit.transform, turns)
    else {
        return false;
    };
    placement.reserve(edit.cells.len());
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
    for cell in edit.cells.iter().copied() {
        placement.insert_entity_into_cell(cell, spawned);
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
