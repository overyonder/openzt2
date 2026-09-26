use crate::assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use crate::assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::plugins::animal_lifecycle::types::SpeciesHandle;
use crate::plugins::guests::guest_simulation_types::GuestArchetype;
use crate::plugins::staff::staff_employment_types::StaffRole;
use arrayvec::ArrayVec;
use bevy::prelude::*;

use super::behavior_move_execution::PendingBehaviorMoveCompletion;
use super::behavior_set_start_request_types::{PendingBehaviorSet, StartBehaviorSet};
use crate::plugins::behavior_task_execution_types::{
    BehaviorTaskExecutionPhase, BehaviorTaskExecutionState, BehaviorTaskFailed,
    PendingBehaviorAnimationClipCompletion, PendingBehaviorDockingCompletion,
    PendingBehaviorTaskFailure,
};
use crate::plugins::locomotion::locomotion_types::{Destination, Docking};

pub(super) fn accept_animal_behavior_set_start_requests(
    mut behavior_set_start_requests: MessageReader<StartBehaviorSet>,
    mut commands: Commands,
    synchronized_partners: Query<
        (),
        With<super::behavior_synchronized_set_execution::SynchronizedBehaviorParticipant>,
    >,
) {
    for behavior_set_start_request in behavior_set_start_requests.read() {
        if synchronized_partners.contains(behavior_set_start_request.actor) {
            continue;
        }
        commands
            .entity(behavior_set_start_request.actor)
            .insert(PendingBehaviorSet {
                program: behavior_set_start_request.program,
                target: behavior_set_start_request.target,
            });
    }
}

pub(super) fn resolve_pending_animal_behavior_sets_to_loaded_declarations(
    loaded_behavior_documents: Res<LoadedBehaviorDocumentCollection>,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    pending_behavior_sets: Query<(
        Entity,
        &PendingBehaviorSet,
        Option<&SpeciesHandle>,
        Option<&StaffRole>,
        Option<&GuestArchetype>,
        Has<BehaviorTaskExecutionState>,
        Option<&PendingBehaviorDockingCompletion>,
        Option<&PendingBehaviorMoveCompletion>,
    )>,
    navigation: Query<(Option<&Destination>, Option<&Docking>)>,
    mut commands: Commands,
    mut failed_tasks: MessageWriter<BehaviorTaskFailed>,
    mut execution_sequence: ResMut<
        crate::plugins::behavior_task_execution_types::BehaviorTaskExecutionSequence,
    >,
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
        has_task,
        pending_dock,
        pending_move,
    ) in &pending_behavior_sets
    {
        let subject_type_identifiers =
            super::behavior_subject_type_resolution::behavior_subject_type_identifiers(
                species_handle,
                staff,
                guest,
                definitions,
            );
        let Some((behavior_document_handle, declaration_index)) = behavior_declaration_index
            .find_behavior_set_location(pending_behavior_set.program, subject_type_identifiers)
        else {
            error!(actor = ?actor_entity, program = ?pending_behavior_set.program,
                "requested behavior set has no matching declaration in the settled archive index");
            commands.entity(actor_entity).remove::<PendingBehaviorSet>();
            if has_task {
                super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(actor_entity, &mut commands);
            } else {
                failed_tasks.write(BehaviorTaskFailed {
                    actor: actor_entity,
                    execution_id: execution_sequence.next(),
                });
            }
            continue;
        };
        if let Some(request_id) = pending_dock
            .map(|pending| pending.request_id)
            .or_else(|| pending_move.map(|pending| pending.request_id))
        {
            if let Ok((destination, docking)) = navigation.get(actor_entity) {
                if destination.is_some_and(|value| value.request_id == request_id) {
                    commands.entity(actor_entity).remove::<Destination>();
                }
                if docking.is_some_and(|value| value.request_id == request_id) {
                    commands.entity(actor_entity).remove::<Docking>();
                }
            }
        }
        commands
            .entity(actor_entity)
            .insert(BehaviorTaskExecutionState {
                execution_id: execution_sequence.next(),
                program: pending_behavior_set.program,
                target: pending_behavior_set.target,
                document: behavior_document_handle,
                declaration: declaration_index,
                phase: BehaviorTaskExecutionPhase::Set,
                action: 0,
                repetitions: 0,
                next_action_tick: 0,
                interaction_slot: None,
                stack: ArrayVec::new(),
            })
            .remove::<(
                PendingBehaviorAnimationClipCompletion,
                PendingBehaviorMoveCompletion,
                PendingBehaviorDockingCompletion,
                PendingBehaviorTaskFailure,
            )>()
            .remove::<PendingBehaviorSet>();
    }
}
