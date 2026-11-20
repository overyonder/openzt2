//! Routes each prepared placement edit to one application procedure and acknowledges it once.

use bevy::gltf::Gltf;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprints;
use openzt2_zoo_state::placement::placed_object_types::IndexedObjectPlacementCells;

use openzt2_game_data::world_definitions::object_placement::placement_stomp_rules::PlacementStompAction;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::placement::object_placement_validation::object_placement_stomp_action;

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionPlacementPolicy;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use crate::placement::object_placement_edit_direction::object_placement_edit_application_creates_entity;
use crate::placement::object_placement_request_routing::acknowledge_object_placement_edit_application;
use crate::placement::placed_object_creation_edit_application::apply_prepared_placed_object_creation_edit;
use crate::placement::placed_object_relocation_edit_application::apply_prepared_placed_object_relocation_edit;
use crate::placement::placed_object_removal_edit_application::apply_prepared_placed_object_removal_edit;
use openzt2_zoo_state::placement::object_placement_definition_queries::collect_occupied_placement_cells_for_transform;
use openzt2_zoo_state::placement::object_placement_definition_queries::resolve_object_placeable_definition;
use openzt2_zoo_state::placement::object_placement_validation::calculate_authored_object_placement_eighth_turns;
use openzt2_zoo_state::placement::object_placement_validation::calculate_object_placement_footprint_origin;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectFootprintOccupancy;
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementCellCollectionScratch;
use openzt2_zoo_state::placement::placement_transaction_types::ApplyPreparedObjectPlacementEditRequest;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementCommitted;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditApplicationAcknowledged;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementEdit;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementEditMutationKind;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementMutation;

fn ordered_prepared_object_placement_mutation_indices(
    mutation_count: usize,
    forward: bool,
) -> impl Iterator<Item = usize> {
    (0..mutation_count).map(move |index| {
        if forward {
            index
        } else {
            mutation_count - 1 - index
        }
    })
}

/// Routes each accepted edit to its mutation-specific application procedure.
pub(super) fn apply_prepared_object_placement_edits(
    mut commands: Commands,
    mut requests: MessageReader<ApplyPreparedObjectPlacementEditRequest>,
    mut edits: Query<&mut PreparedObjectPlacementEdit>,
    roots: Query<Entity, With<WorldRoot>>,
    mut placement: ResMut<PlacedObjectFootprintOccupancyIndex>,
    mut scratch: ResMut<ObjectPlacementCellCollectionScratch>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    (prefabs, footprints): (Res<Assets<ScenePrefabAsset>>, Res<ScenePlacementFootprints>),
    models: Res<Assets<Gltf>>,
    mut placed: Query<(
        &PersistentId,
        &PlacedObjectDefinitionReference,
        &mut Transform,
        &mut PlacedObjectFootprintOccupancy,
    )>,
    mut indexed_cells: Query<&mut IndexedObjectPlacementCells>,
    policy: Res<ConstructionPlacementPolicy>,
    mut tool: ResMut<ConstructionTool>,
    mut object_placed: MessageWriter<ObjectPlacementCommitted>,
    mut acknowledged: MessageWriter<ObjectPlacementEditApplicationAcknowledged>,
) {
    let catalogue = active_definitions.get(&definitions);
    for request in requests.read() {
        let accepted = catalogue
            .and_then(|catalogue| {
                let mut edit = edits.get_mut(request.transaction).ok()?;
                if !preflight_prepared_object_placement_mutations(
                    request,
                    &edit,
                    catalogue,
                    roots.single().ok(),
                    &placement,
                    &mut scratch,
                    &footprints,
                    &prefabs,
                    &models,
                    &placed,
                ) {
                    return Some(false);
                }
                let forward = matches!(
                    request.application,
                    openzt2_zoo_state::construction::construction_transaction_types::EditApplication::InitialCommit
                        | openzt2_zoo_state::construction::construction_transaction_types::EditApplication::Redo
                );
                let mutation_indices = ordered_prepared_object_placement_mutation_indices(
                    edit.mutations.len(),
                    forward,
                );
                let mut applied_mutation_indices = Vec::with_capacity(edit.mutations.len());
                let mut pending_created_definitions = HashMap::new();
                let mut accepted = true;
                for mutation_index in mutation_indices {
                    if apply_prepared_object_placement_mutation(
                            &mut commands,
                            request,
                            &mut edit.mutations[mutation_index],
                            catalogue,
                            roots.single().ok(),
                            &mut placement,
                            &mut scratch,
                            &footprints,
                            &prefabs,
                            &models,
                            &mut placed,
                            &mut indexed_cells,
                            &mut pending_created_definitions,
                            policy.repeat_placement,
                            &mut tool,
                            &mut object_placed,
                        ) {
                        applied_mutation_indices.push(mutation_index);
                    } else {
                        accepted = false;
                        break;
                    }
                }
                if !accepted {
                    let rollback_request = ApplyPreparedObjectPlacementEditRequest {
                        transaction: request.transaction,
                        application: openzt2_zoo_state::construction::construction_transaction_types::EditApplication::FailureRollback,
                    };
                    for mutation_index in applied_mutation_indices.into_iter().rev() {
                        let rolled_back = apply_prepared_object_placement_mutation(
                            &mut commands,
                            &rollback_request,
                            &mut edit.mutations[mutation_index],
                            catalogue,
                            roots.single().ok(),
                            &mut placement,
                            &mut scratch,
                            &footprints,
                            &prefabs,
                            &models,
                            &mut placed,
                            &mut indexed_cells,
                            &mut pending_created_definitions,
                            policy.repeat_placement,
                            &mut tool,
                            &mut object_placed,
                        );
                        debug_assert!(rolled_back, "accepted placement mutation must roll back");
                    }
                }
                Some(accepted)
            })
            .unwrap_or(false);
        acknowledge_object_placement_edit_application(&mut acknowledged, request, accepted);
    }
}

#[allow(clippy::too_many_arguments)]
fn preflight_prepared_object_placement_mutations(
    request: &ApplyPreparedObjectPlacementEditRequest,
    edit: &PreparedObjectPlacementEdit,
    catalogue: openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>,
    root: Option<Entity>,
    placement: &PlacedObjectFootprintOccupancyIndex,
    scratch: &mut ObjectPlacementCellCollectionScratch,
    footprints: &ScenePlacementFootprints,
    prefabs: &Assets<ScenePrefabAsset>,
    models: &Assets<Gltf>,
    placed: &Query<(
        &PersistentId,
        &PlacedObjectDefinitionReference,
        &mut Transform,
        &mut PlacedObjectFootprintOccupancy,
    )>,
) -> bool {
    let forward = matches!(
        request.application,
        openzt2_zoo_state::construction::construction_transaction_types::EditApplication::InitialCommit
            | openzt2_zoo_state::construction::construction_transaction_types::EditApplication::Redo
    );
    let indices = ordered_prepared_object_placement_mutation_indices(edit.mutations.len(), forward);
    let mut removed_entities = bevy::platform::collections::HashSet::new();
    let mut projected_created_cells = HashMap::new();
    for index in indices {
        let mutation = &edit.mutations[index];
        let target = find_prepared_object_placement_mutation_entity(mutation, placement, placed);
        if mutation.mutation == PreparedObjectPlacementEditMutationKind::Relocate {
            let Some(entity) = target else {
                return false;
            };
            removed_entities.insert(entity);
            continue;
        }
        let creates = object_placement_edit_application_creates_entity(
            mutation.mutation,
            request.application,
            target.is_some(),
        );
        if creates {
            let Some(definition) =
                resolve_object_placeable_definition(catalogue, mutation.definition)
            else {
                return false;
            };
            let geometry = footprints.resolve(definition, Some(&mutation.prefab));
            if root.is_none()
                || collect_occupied_placement_cells_for_transform(
                    geometry,
                    &mutation.transform,
                    &mut scratch.cells,
                )
                .is_none_or(|cells| cells != mutation.cells.as_ref())
                || calculate_authored_object_placement_eighth_turns(definition, &mutation.transform)
                    .and_then(|turns| {
                        calculate_object_placement_footprint_origin(
                            geometry,
                            &mutation.transform,
                            turns,
                        )
                    })
                    .is_none()
                || prefabs.get(&mutation.prefab).is_none_or(|prefab| {
                    first_missing_prefab_collider_model_asset_id(prefab, models).is_some()
                })
            {
                return false;
            }
            for cell in mutation.cells.iter().copied() {
                let existing_blocked = placement
                    .entities_occupying_cell(cell)
                    .filter(|entity| !removed_entities.contains(entity))
                    .any(|entity| {
                        let target = placed
                            .get(entity)
                            .ok()
                            .map(|(_, reference, _, _)| reference.definition);
                        object_placement_stomp_action(definition, catalogue, target)
                            != PlacementStompAction::Allow
                    });
                let projected_blocked = projected_created_cells.get(&cell).is_some_and(|target| {
                    object_placement_stomp_action(definition, catalogue, Some(*target))
                        != PlacementStompAction::Allow
                });
                if existing_blocked || projected_blocked {
                    return false;
                }
                projected_created_cells.insert(cell, mutation.definition);
            }
        } else {
            let Some(target) = target else {
                return false;
            };
            let valid = placed
                .get(target)
                .ok()
                .and_then(|(id, reference, transform, _)| {
                    let definition =
                        resolve_object_placeable_definition(catalogue, reference.definition)?;
                    let exact_cells = collect_occupied_placement_cells_for_transform(
                        footprints.resolve(definition, Some(&mutation.prefab)),
                        transform,
                        &mut scratch.cells,
                    )
                    .is_some_and(|cells| cells == mutation.cells.as_ref());
                    Some(
                        *id == mutation.entity
                            && reference.definition == mutation.definition
                            && exact_cells
                            && mutation
                                .cells
                                .iter()
                                .all(|cell| placement.cell_contains_entity(*cell, target)),
                    )
                })
                .unwrap_or(false);
            if !valid {
                return false;
            }
            removed_entities.insert(target);
        }
    }
    true
}

fn find_prepared_object_placement_mutation_entity(
    edit: &PreparedObjectPlacementMutation,
    placement: &PlacedObjectFootprintOccupancyIndex,
    placed: &Query<(
        &PersistentId,
        &PlacedObjectDefinitionReference,
        &mut Transform,
        &mut PlacedObjectFootprintOccupancy,
    )>,
) -> Option<Entity> {
    edit.applied_entity
        .filter(|entity| placed.get(*entity).is_ok())
        .or_else(|| {
            edit.cells.first().and_then(|cell| {
                placement.entities_occupying_cell(*cell).find(|entity| {
                    placed
                        .get(*entity)
                        .is_ok_and(|(id, _, _, _)| *id == edit.entity)
                })
            })
        })
}

#[allow(clippy::too_many_arguments)]
fn apply_prepared_object_placement_mutation(
    commands: &mut Commands,
    request: &ApplyPreparedObjectPlacementEditRequest,
    edit: &mut PreparedObjectPlacementMutation,
    catalogue: openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>,
    root: Option<Entity>,
    placement: &mut PlacedObjectFootprintOccupancyIndex,
    scratch: &mut ObjectPlacementCellCollectionScratch,
    footprints: &ScenePlacementFootprints,
    prefabs: &Assets<ScenePrefabAsset>,
    models: &Assets<Gltf>,
    placed: &mut Query<(
        &PersistentId,
        &PlacedObjectDefinitionReference,
        &mut Transform,
        &mut PlacedObjectFootprintOccupancy,
    )>,
    indexed_cells: &mut Query<&mut IndexedObjectPlacementCells>,
    pending_created_definitions: &mut HashMap<Entity, AssetId>,
    repeat_placement: bool,
    tool: &mut ConstructionTool,
    object_placed: &mut MessageWriter<ObjectPlacementCommitted>,
) -> bool {
    if edit.mutation == PreparedObjectPlacementEditMutationKind::Relocate {
        let accepted = apply_prepared_placed_object_relocation_edit(
            commands,
            request.application,
            edit,
            catalogue,
            placement,
            scratch,
            footprints,
            indexed_cells,
            placed,
        );
        if accepted && request.application == EditApplication::InitialCommit {
            *tool = ConstructionTool::Inspect;
        }
        return accepted;
    }

    let entity_exists = edit
        .applied_entity
        .is_some_and(|entity| placed.get(entity).is_ok())
        || edit.cells.first().is_some_and(|cell| {
            placement.entities_occupying_cell(*cell).any(|entity| {
                placed
                    .get(entity)
                    .is_ok_and(|(id, _, _, _)| *id == edit.entity)
            })
        });
    if object_placement_edit_application_creates_entity(
        edit.mutation,
        request.application,
        entity_exists,
    ) {
        let accepted = apply_prepared_placed_object_creation_edit(
            commands,
            request.transaction,
            edit,
            catalogue,
            root,
            placement,
            scratch,
            footprints,
            prefabs,
            models,
            &placed.as_readonly(),
            pending_created_definitions,
            &[],
            repeat_placement,
            tool,
            object_placed,
        );
        if accepted {
            if let Some(entity) = edit.applied_entity {
                pending_created_definitions.insert(entity, edit.definition);
            }
        }
        accepted
    } else {
        if let Some(entity) = edit.applied_entity {
            pending_created_definitions.remove(&entity);
        }
        apply_prepared_placed_object_removal_edit(
            commands, edit, catalogue, placement, scratch, footprints, placed,
        )
    }
}
