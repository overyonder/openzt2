//! Routes each prepared placement edit to one application procedure and acknowledges it once.

use bevy::{gltf::Gltf, platform::collections::HashMap, prelude::*};
use openzt2_game_data::{
    world_definitions::{object_placement::PlacementConstraints, world_objects::WorldObjectKind},
    AssetId,
};

use crate::assets::scene_prefab::ScenePrefabAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::construction::construction_tool_and_placement_policy_types::ConstructionPlacementPolicy;
use crate::plugins::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use crate::plugins::world_spawn::persistent_id_types::PersistentId;
use crate::plugins::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::{
    object_placement_definition_queries::{
        collect_occupied_placement_cells_for_transform, resolve_object_placeable_definition,
    },
    object_placement_edit_direction::object_placement_edit_application_creates_entity,
    object_placement_request_routing::acknowledge_object_placement_edit_application,
    object_placement_validation::{
        calculate_authored_object_placement_eighth_turns,
        calculate_object_placement_footprint_origin,
    },
    placed_object_creation_edit_application::apply_prepared_placed_object_creation_edit,
    placed_object_relocation_edit_application::apply_prepared_placed_object_relocation_edit,
    placed_object_removal_edit_application::apply_prepared_placed_object_removal_edit,
    placed_object_types::{PlacedObjectDefinitionReference, PlacedObjectFootprintOccupancy},
    placement_occupancy_index::PlacedObjectFootprintOccupancyIndex,
    placement_preview_types::ObjectPlacementCellCollectionScratch,
    placement_transaction_types::{
        ApplyPreparedObjectPlacementEditRequest, ObjectPlacementCommitted,
        ObjectPlacementEditApplicationAcknowledged, PreparedObjectPlacementEdit,
        PreparedObjectPlacementEditMutationKind, PreparedObjectPlacementMutation,
    },
};

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
    prefabs: Res<Assets<ScenePrefabAsset>>,
    models: Res<Assets<Gltf>>,
    mut placed: Query<(
        &PersistentId,
        &PlacedObjectDefinitionReference,
        &mut Transform,
        &mut PlacedObjectFootprintOccupancy,
    )>,
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
                    &prefabs,
                    &models,
                    &placed,
                ) {
                    return Some(false);
                }
                let forward = matches!(
                    request.application,
                    crate::plugins::construction::construction_transaction_types::EditApplication::InitialCommit
                        | crate::plugins::construction::construction_transaction_types::EditApplication::Redo
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
                            &prefabs,
                            &models,
                            &mut placed,
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
                        application: crate::plugins::construction::construction_transaction_types::EditApplication::FailureRollback,
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
                            &prefabs,
                            &models,
                            &mut placed,
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
    catalogue: crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>,
    root: Option<Entity>,
    placement: &PlacedObjectFootprintOccupancyIndex,
    scratch: &mut ObjectPlacementCellCollectionScratch,
    prefabs: &Assets<ScenePrefabAsset>,
    models: &Assets<Gltf>,
    placed: &Query<(
        &PersistentId,
        &PlacedObjectDefinitionReference,
        &mut Transform,
        &mut PlacedObjectFootprintOccupancy,
    )>,
) -> bool {
    if edit.mutations.len() == 1
        && edit.mutations[0].mutation == PreparedObjectPlacementEditMutationKind::Relocate
    {
        return true;
    }
    if edit
        .mutations
        .iter()
        .any(|mutation| mutation.mutation == PreparedObjectPlacementEditMutationKind::Relocate)
    {
        return false;
    }
    let forward = matches!(
        request.application,
        crate::plugins::construction::construction_transaction_types::EditApplication::InitialCommit
            | crate::plugins::construction::construction_transaction_types::EditApplication::Redo
    );
    let indices = ordered_prepared_object_placement_mutation_indices(edit.mutations.len(), forward);
    let mut removed_entities = bevy::platform::collections::HashSet::new();
    let mut projected_created_cells = HashMap::new();
    for index in indices {
        let mutation = &edit.mutations[index];
        let target = find_prepared_object_placement_mutation_entity(mutation, placement, placed);
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
            if root.is_none()
                || collect_occupied_placement_cells_for_transform(
                    definition,
                    &mutation.transform,
                    &mut scratch.cells,
                )
                .is_none_or(|cells| cells != mutation.cells.as_ref())
                || calculate_authored_object_placement_eighth_turns(definition, &mutation.transform)
                    .and_then(|turns| {
                        calculate_object_placement_footprint_origin(
                            definition,
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
            let allows_scenery = definition
                .constraints
                .contains_all(PlacementConstraints::ALLOW_OVERLAP_SCENERY);
            for cell in mutation.cells.iter().copied() {
                let existing_blocked = placement
                    .entities_occupying_cell(cell)
                    .filter(|entity| !removed_entities.contains(entity))
                    .any(|entity| {
                        !allows_scenery
                            || placed
                                .get(entity)
                                .ok()
                                .and_then(|(_, reference, _, _)| {
                                    catalogue.find_object(reference.definition)
                                })
                                .is_none_or(|object| {
                                    !matches!(object.kind, WorldObjectKind::Scenery)
                                })
                    });
                let projected_blocked =
                    projected_created_cells
                        .get(&cell)
                        .is_some_and(|definition_id| {
                            !allows_scenery
                                || catalogue.find_object(*definition_id).is_none_or(|object| {
                                    !matches!(object.kind, WorldObjectKind::Scenery)
                                })
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
                        definition,
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
    catalogue: crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>,
    root: Option<Entity>,
    placement: &mut PlacedObjectFootprintOccupancyIndex,
    scratch: &mut ObjectPlacementCellCollectionScratch,
    prefabs: &Assets<ScenePrefabAsset>,
    models: &Assets<Gltf>,
    placed: &mut Query<(
        &PersistentId,
        &PlacedObjectDefinitionReference,
        &mut Transform,
        &mut PlacedObjectFootprintOccupancy,
    )>,
    pending_created_definitions: &mut HashMap<Entity, AssetId>,
    repeat_placement: bool,
    tool: &mut ConstructionTool,
    object_placed: &mut MessageWriter<ObjectPlacementCommitted>,
) -> bool {
    if edit.mutation == PreparedObjectPlacementEditMutationKind::Relocate {
        return apply_prepared_placed_object_relocation_edit(
            commands,
            request.application,
            edit,
            catalogue,
            placement,
            scratch,
            placed,
        );
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
            prefabs,
            models,
            placed,
            pending_created_definitions,
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
            commands, edit, catalogue, placement, scratch, placed,
        )
    }
}
