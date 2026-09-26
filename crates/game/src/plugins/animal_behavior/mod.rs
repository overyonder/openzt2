//! Live execution of typed per-document behavior declarations.

mod autonomous_task_selection;
mod behavior_animation_clip_execution;
pub(crate) mod behavior_candidate_selection;
mod behavior_docking_execution;
pub(crate) mod behavior_entity_role_resolution;
mod behavior_fact_modification;
mod behavior_feedback_dispatch_execution;
mod behavior_interaction_container_execution;
pub(crate) mod behavior_move_execution;
mod behavior_object_attachment_execution;
mod behavior_queue_wait_execution;
mod behavior_random_animation_execution;
mod behavior_random_choice_execution;
mod behavior_random_stream_initialization;
pub(crate) mod behavior_random_stream_state;
mod behavior_script_execution;
mod behavior_set_start_request_handling;
pub(crate) mod behavior_set_start_request_types;
mod behavior_subject_type_resolution;
pub(crate) mod behavior_synchronized_set_execution;
mod behavior_target_test_execution;
pub(crate) mod behavior_task_failure_transition;
mod behavior_task_phase_transition_execution;
pub(crate) mod interaction_container_occupancy;

use bevy::prelude::*;

use crate::{
    application_lifecycle::GamePhase, application_schedule::FixedGameSet,
    plugins::simulation_time::deterministic_random_stream::ZooSeed,
};

use crate::plugins::behavior_task_execution_types::{
    BehaviorFeedbackDispatched, BehaviorTaskFailed, BehaviorTaskFinished,
};
use behavior_set_start_request_types::StartBehaviorSet;

pub(crate) struct AnimalBehaviorExecutionPlugin;

impl Plugin for AnimalBehaviorExecutionPlugin {
    fn build(&self, application: &mut App) {
        behavior_object_attachment_execution::register_object_attachment_execution(application);
        behavior_queue_wait_execution::register_queue_wait_execution(application);
        behavior_synchronized_set_execution::register_synchronized_behavior_execution(application);
        application
            .init_non_send::<behavior_script_execution::BehaviorScriptContexts>()
            .init_resource::<crate::plugins::behavior_task_execution_types::BehaviorTaskExecutionSequence>()
            .init_resource::<interaction_container_occupancy::InteractionContainerOccupancy>()
            .add_message::<StartBehaviorSet>()
            .add_message::<BehaviorFeedbackDispatched>()
            .add_message::<BehaviorTaskFailed>()
            .add_message::<BehaviorTaskFinished>()
            .add_systems(
                FixedUpdate,
                behavior_random_stream_initialization::attach_behavior_random_stream_to_animals_and_staff
                    .in_set(FixedGameSet::Clock)
                    .run_if(in_state(GamePhase::InGame))
                    .run_if(resource_exists::<ZooSeed>),
            )
            .add_systems(
                Update,
                (
                    behavior_script_execution::invalidate_changed_behavior_script_contexts,
                    behavior_set_start_request_handling::accept_animal_behavior_set_start_requests,
                    behavior_set_start_request_handling::resolve_pending_animal_behavior_sets_to_loaded_declarations,
                )
                    .chain()
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                autonomous_task_selection::select_idle_animal_tasks
                    .after(crate::plugins::feeding::ContainerQuantityHydration)
                    .before(behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
                    .in_set(FixedGameSet::Think)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome
                    .in_set(FixedGameSet::Think)
                    .run_if(in_state(GamePhase::InGame)),
            );
        application.add_systems(
            FixedUpdate,
            (
                behavior_feedback_dispatch_execution::dispatch_current_authored_behavior_feedback_and_advance,
                interaction_container_occupancy::remove_destroyed_container_memberships,
                behavior_interaction_container_execution::enter_authored_interaction_containers_and_play_selected_behavior_sets,
                behavior_docking_execution::start_current_supported_behavior_docking_actions,
                behavior_move_execution::start_current_supported_behavior_move_actions,
                behavior_animation_clip_execution::start_current_behavior_animation_clip_actions,
                behavior_random_animation_execution::start_next_random_behavior_animation,
                behavior_fact_modification::apply_supported_current_behavior_fact_modifications,
                behavior_script_execution::execute_authored_behavior_script_actions,
                behavior_target_test_execution::find_target_and_enter_success_or_failure_behavior_set,
            )
                .chain()
                .after(
                    behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome,
                )
                .after(crate::plugins::feeding::ContainerQuantityHydration)
                .in_set(FixedGameSet::Think)
                .run_if(in_state(GamePhase::InGame)),
        );
        application.add_systems(
            FixedUpdate,
            (
                (
                    behavior_docking_execution::fail_behavior_docking_actions_after_navigation_failure,
                    behavior_docking_execution::finish_behavior_docking_actions_after_matching_arrival,
                )
                    .chain(),
                (
                    behavior_move_execution::fail_behavior_move_actions_after_navigation_failure,
                    behavior_move_execution::finish_behavior_move_actions_after_matching_arrival,
                )
                    .chain(),
                (
                    behavior_animation_clip_execution::fail_behavior_animation_actions_after_playback_rejection,
                    behavior_animation_clip_execution::finish_behavior_animation_clip_actions_after_playback_completion,
                ).chain(),
            )
                .in_set(FixedGameSet::Act)
                .run_if(in_state(GamePhase::InGame)),
        );
    }
}
