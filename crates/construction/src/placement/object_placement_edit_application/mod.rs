//! Routes each prepared placement edit to one application procedure and acknowledges it once.

use bevy::gltf::Gltf;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprints;

use openzt2_game_data::world_definitions::object_placement::placement_stomp_rules::PlacementStompAction;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::placement::object_placement_validation::object_placement_stomp_action;

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionPlacementPolicy;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::construction::construction_transaction_types::{EditApplication, RollbackContext};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use crate::placement::object_placement_edit_direction::object_placement_edit_application_creates_entity;
use crate::placement::placed_object_creation_edit_application::apply_prepared_placed_object_creation_edit;
use crate::placement::placed_object_relocation_edit_application::apply_prepared_placed_object_relocation_edit;
use crate::placement::placed_object_removal_edit_application::apply_prepared_placed_object_removal_edit;
use openzt2_zoo_state::placement::object_placement_definition_queries::collect_occupied_placement_cells_for_transform;
use openzt2_zoo_state::placement::object_placement_definition_queries::resolve_object_placeable_definition;
use openzt2_zoo_state::placement::object_placement_validation::calculate_authored_object_placement_eighth_turns;
use openzt2_zoo_state::placement::object_placement_validation::calculate_object_placement_footprint_origin;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectFootprintOccupancy;
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintQueries;
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

/// Materialize each ordered mutation before the next reads the live footprint.
pub(super) fn apply_prepared_object_placement_edits(
    mut commands: Commands,
    mut requests: MessageReader<ApplyPreparedObjectPlacementEditRequest>,
) {
    for request in requests.read().copied() {
        commands.queue(move |world: &mut World| {
            let accepted = apply_placement_batch(world, request);
            world.write_message(ObjectPlacementEditApplicationAcknowledged {
                transaction: request.transaction,
                application: request.application,
                accepted,
            });
        });
    }
}

fn apply_placement_batch(
    world: &mut World,
    request: ApplyPreparedObjectPlacementEditRequest,
) -> bool {
    let Some(indices) = world
        .run_system_cached_with(preflight_placement_batch, request)
        .ok()
        .flatten()
    else {
        return false;
    };
    let mut applied = Vec::with_capacity(indices.len());
    for index in indices {
        if world
            .run_system_cached_with(commit_placement_mutation, (request, index))
            .unwrap_or(false)
        {
            applied.push(index);
            continue;
        }
        let rollback = ApplyPreparedObjectPlacementEditRequest {
            application: EditApplication::FailureRollback,
            ..request
        };
        for index in applied.into_iter().rev() {
            let restored = world
                .run_system_cached_with(commit_placement_mutation, (rollback, index))
                .unwrap_or(false);
            debug_assert!(restored, "accepted placement mutation must roll back");
        }
        return false;
    }
    true
}

fn preflight_placement_batch(
    In(request): In<ApplyPreparedObjectPlacementEditRequest>,
    edits: Query<(
        &PreparedObjectPlacementEdit,
        &WorldMember,
        Option<&RollbackContext>,
    )>,
    placement: PlacedObjectFootprintQueries,
    mut scratch: ResMut<ObjectPlacementCellCollectionScratch>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    (prefabs, footprints): (Res<Assets<ScenePrefabAsset>>, Res<ScenePlacementFootprints>),
    models: Res<Assets<Gltf>>,
    placed: Query<(
        &PersistentId,
        &PlacedObjectDefinitionReference,
        &Transform,
        &PlacedObjectFootprintOccupancy,
    )>,
) -> Option<Vec<usize>> {
    let (edit, member, rollback) = edits.get(request.transaction).ok()?;
    let forward = matches!(
        request.application,
        EditApplication::InitialCommit | EditApplication::Redo
    ) || (request.application == EditApplication::FailureRollback
        && rollback.is_some_and(|rollback| rollback.failed_application == EditApplication::Undo));
    let indices = ordered_prepared_object_placement_mutation_indices(edit.mutations.len(), forward);
    preflight_prepared_object_placement_mutations(
        &request,
        edit,
        forward,
        active_definitions.get(&definitions)?,
        member.root,
        &placement,
        &mut scratch,
        &footprints,
        &prefabs,
        &models,
        &placed,
    )
    .then(|| indices.collect())
}

fn commit_placement_mutation(
    In((request, index)): In<(ApplyPreparedObjectPlacementEditRequest, usize)>,
    mut commands: Commands,
    mut edits: Query<(&mut PreparedObjectPlacementEdit, &WorldMember)>,
    placement: PlacedObjectFootprintQueries,
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
    policy: Res<ConstructionPlacementPolicy>,
    mut tool: ResMut<ConstructionTool>,
    mut object_placed: MessageWriter<ObjectPlacementCommitted>,
) -> bool {
    let (Some(catalogue), Ok((mut edit, member))) = (
        active_definitions.get(&definitions),
        edits.get_mut(request.transaction),
    ) else {
        return false;
    };
    apply_prepared_object_placement_mutation(
        &mut commands,
        &request,
        &mut edit.mutations[index],
        catalogue,
        member.root,
        &placement,
        &mut scratch,
        &footprints,
        &prefabs,
        &models,
        &mut placed,
        policy.repeat_placement,
        &mut tool,
        &mut object_placed,
    )
}

#[allow(clippy::too_many_arguments)]
fn preflight_prepared_object_placement_mutations(
    request: &ApplyPreparedObjectPlacementEditRequest,
    edit: &PreparedObjectPlacementEdit,
    forward: bool,
    catalogue: openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>,
    root: Entity,
    placement: &PlacedObjectFootprintQueries,
    scratch: &mut ObjectPlacementCellCollectionScratch,
    footprints: &ScenePlacementFootprints,
    prefabs: &Assets<ScenePrefabAsset>,
    models: &Assets<Gltf>,
    placed: &Query<(
        &PersistentId,
        &PlacedObjectDefinitionReference,
        &Transform,
        &PlacedObjectFootprintOccupancy,
    )>,
) -> bool {
    let indices = ordered_prepared_object_placement_mutation_indices(edit.mutations.len(), forward);
    let mut removed_entities = bevy::platform::collections::HashSet::new();
    let mut projected_created_cells: HashMap<IVec2, Vec<AssetId>> = HashMap::new();
    for index in indices {
        let mutation = &edit.mutations[index];
        let target =
            find_prepared_object_placement_mutation_entity(mutation, root, placement, placed);
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
            if collect_occupied_placement_cells_for_transform(
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
                    .entities_occupying_cell(root, cell)
                    .filter(|entity| !removed_entities.contains(entity))
                    .any(|entity| {
                        let target = placed
                            .get(entity)
                            .ok()
                            .map(|(_, reference, _, _)| reference.definition);
                        object_placement_stomp_action(definition, catalogue, target)
                            != PlacementStompAction::Allow
                    });
                let projected_blocked = projected_created_cells.get(&cell).is_some_and(|targets| {
                    targets.iter().any(|&target| {
                        object_placement_stomp_action(definition, catalogue, Some(target))
                            != PlacementStompAction::Allow
                    })
                });
                if existing_blocked || projected_blocked {
                    return false;
                }
                projected_created_cells
                    .entry(cell)
                    .or_default()
                    .push(mutation.definition);
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
                                .all(|cell| placement.cell_contains_entity(root, *cell, target)),
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
    root: Entity,
    placement: &PlacedObjectFootprintQueries,
    placed: &Query<(
        &PersistentId,
        &PlacedObjectDefinitionReference,
        &Transform,
        &PlacedObjectFootprintOccupancy,
    )>,
) -> Option<Entity> {
    edit.applied_entity
        .filter(|entity| placed.get(*entity).is_ok())
        .or_else(|| {
            edit.cells.first().and_then(|cell| {
                placement
                    .entities_occupying_cell(root, *cell)
                    .find(|entity| {
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
    root: Entity,
    placement: &PlacedObjectFootprintQueries,
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
            root,
            placement,
            scratch,
            footprints,
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
            placement
                .entities_occupying_cell(root, *cell)
                .any(|entity| {
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
        apply_prepared_placed_object_creation_edit(
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
            &[],
            repeat_placement,
            tool,
            object_placed,
        )
    } else {
        apply_prepared_placed_object_removal_edit(
            commands, edit, catalogue, root, placement, scratch, footprints, placed,
        )
    }
}
