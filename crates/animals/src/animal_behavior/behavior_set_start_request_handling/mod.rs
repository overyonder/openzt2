use openzt2_zoo_state::behavior_task_execution_types::running_actions::{BehaviorTaskReplaced};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::guests::guest_simulation_types::GuestArchetype;
use openzt2_zoo_state::staff::staff_employment_types::StaffRole;
use bevy::{ecs::entity::EntityHashMap, prelude::*};

use super::behavior_set_start_request_types::{PendingBehaviorSet, StartBehaviorSet};
use openzt2_zoo_state::animal_behavior::behavior_pursuit_types::PendingBehaviorPursuit;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFailed;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorAnimationClipCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorDockingCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorMoveCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::behavior_task_execution_types::{
    allocate_behavior_execution_id, BehaviorTaskGeneration,
};
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::Docking;

pub(super) fn accept_animal_behavior_set_start_requests(
    mut behavior_set_start_requests: MessageReader<StartBehaviorSet>,
    mut commands: Commands,
    mut pending: Query<&mut PendingBehaviorSet>,
    synchronized_partners: Query<
        (),
        With<openzt2_zoo_state::behavior_task_execution_types::SynchronizedBehaviorParticipant>,
    >,
) {
    let mut new_request_indices: EntityHashMap<usize> = EntityHashMap::default();
    let mut new_requests: Vec<(Entity, PendingBehaviorSet)> = Vec::new();
    for request in behavior_set_start_requests.read() {
        if synchronized_partners.contains(request.actor) {
            continue;
        }
        let replacement = PendingBehaviorSet {
            program: request.program,
            target: request.target,
        };
        if let Ok(mut current) = pending.get_mut(request.actor) {
            *current = replacement;
        } else if let Some(&index) = new_request_indices.get(&request.actor) {
            new_requests[index] = (request.actor, replacement);
        } else {
            new_request_indices.insert(request.actor, new_requests.len());
            new_requests.push((request.actor, replacement));
        }
    }
    for (actor, request) in new_requests {
        commands.entity(actor).try_insert(request);
    }
}

pub(super) fn resolve_pending_animal_behavior_sets_to_loaded_declarations(
    loaded_behavior_documents: Res<LoadedBehaviorDocumentCollection>,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    animal_types: super::behavior_subject_type_resolution::AnimalBehaviorSubjectTypes,
    pending_behavior_sets: Query<(
        Entity,
        &PendingBehaviorSet,
        Option<&SpeciesHandle>,
        Option<&StaffRole>,
        Option<&GuestArchetype>,
        Option<&PendingBehaviorDockingCompletion>,
        Option<&PendingBehaviorMoveCompletion>,
        Option<&PendingBehaviorPursuit>,
    )>,
    mut tasks: Query<&mut BehaviorTaskExecutionState>,
    navigation: Query<(Option<&Destination>, Option<&Docking>)>,
    mut commands: Commands,
    mut failed_tasks: MessageWriter<BehaviorTaskFailed>,
    mut execution_generations: Query<&mut BehaviorTaskGeneration>,
) {
    let Some(behavior_declaration_index) =
        loaded_behavior_documents.create_declaration_index_view(&behavior_document_assets)
    else {
        return;
    };
    let Some(definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (
        actor_entity,
        pending_behavior_set,
        species_handle,
        staff,
        guest,
        pending_dock,
        pending_move,
        pending_pursuit,
    ) in &pending_behavior_sets
    {
        let subject_type_identifiers =
            super::behavior_subject_type_resolution::behavior_subject_type_identifiers(
                species_handle,
                staff,
                guest,
                definitions,
                animal_types.borrow_for(actor_entity),
            );
        let Some((behavior_document_handle, declaration_index)) = behavior_declaration_index
            .find_behavior_set_location(pending_behavior_set.program, subject_type_identifiers)
        else {
            error!(actor = ?actor_entity, program = ?pending_behavior_set.program,
                "requested behavior set has no matching declaration in the settled archive index");
            commands.entity(actor_entity).remove::<PendingBehaviorSet>();
            if tasks.contains(actor_entity) {
                super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(actor_entity, &mut commands);
            } else {
                failed_tasks.write(BehaviorTaskFailed {
                    actor: actor_entity,
                    execution_id: allocate_behavior_execution_id(
                        actor_entity,
                        execution_generations.get_mut(actor_entity).ok(),
                        &mut commands,
                    ),
                });
            }
            continue;
        };
        let [pursuit_request_id, pursuit_route_request_id] =
            pending_pursuit.map_or([None, None], |pending| pending.leg.navigation_request_ids());
        let owned_request_ids = [
            pending_dock.map(|pending| pending.request_id),
            pending_move.map(|pending| pending.request_id),
            pursuit_request_id,
            pursuit_route_request_id,
        ];
        if let Ok((destination, docking)) = navigation.get(actor_entity) {
            if destination.is_some_and(|value| owned_request_ids.contains(&Some(value.request_id)))
            {
                commands.entity(actor_entity).remove::<Destination>();
            }
            if docking.is_some_and(|value| owned_request_ids.contains(&Some(value.request_id))) {
                commands.entity(actor_entity).remove::<Docking>();
            }
        }
        let replacement = BehaviorTaskExecutionState {
            execution_id: allocate_behavior_execution_id(
                actor_entity,
                execution_generations.get_mut(actor_entity).ok(),
                &mut commands,
            ),
            program: pending_behavior_set.program,
            target: pending_behavior_set.target,
            document: behavior_document_handle,
            declaration: declaration_index,
            phase: BehaviorTaskExecutionPhase::Set,
            action: 0,
            repetitions: 0,
            next_action_tick: 0,
            object: None,
            interaction_slot: None,
            depth: 0,
        };
        commands.entity(actor_entity)
            .remove::<(
                openzt2_zoo_state::animal_behavior::behavior_task_effective_score_types::BehaviorTaskEffectiveScore,
                PendingBehaviorAnimationClipCompletion,
                PendingBehaviorMoveCompletion,
                PendingBehaviorDockingCompletion,
                PendingBehaviorPursuit,
                PendingBehaviorTaskFailure,
            )>()
            .remove::<PendingBehaviorSet>();
        if let Ok(mut current) = tasks.get_mut(actor_entity) {
            let replaced = BehaviorTaskReplaced {
                actor: actor_entity,
                previous_execution: current.execution_id,
                previous_object: current.object,
                execution: replacement.execution_id,
            };
            *current = replacement;
            commands.trigger(replaced);
        } else {
            commands.entity(actor_entity).insert(replacement);
        }
    }
}
