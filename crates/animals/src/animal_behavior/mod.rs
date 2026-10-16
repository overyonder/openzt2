//! Live execution of typed per-document behavior declarations.

mod autonomous_task_selection;
mod behavior_animation_clip_execution;
use openzt2_zoo_state::animal_behavior::behavior_candidate_selection;
mod behavior_clothing_delivery;
mod behavior_docking_execution;
use openzt2_zoo_state::animal_behavior::behavior_entity_role_resolution;
mod behavior_fact_modification;
mod behavior_feedback_dispatch_execution;
mod behavior_head_look_execution;
mod behavior_hide_execution;
mod behavior_interaction_container_execution;
mod behavior_locomotion_switch_set_execution;
pub(crate) mod behavior_move_execution;
mod behavior_object_attachment_execution;
mod behavior_object_spawn_execution;
mod behavior_queue_wait_execution;
mod behavior_random_animation_execution;
mod behavior_random_choice_execution;
mod behavior_random_stream_initialization;
use openzt2_zoo_state::animal_behavior::behavior_random_stream_state;
mod behavior_script_execution;
mod behavior_set_start_request_handling;
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types;
mod behavior_show_event_execution;
mod behavior_subject_type_resolution;
pub(crate) mod behavior_synchronized_set_execution;
pub(crate) mod behavior_target_placement_execution;
use openzt2_zoo_state::animal_behavior::behavior_target_placement_types;
mod behavior_target_test_execution;
use openzt2_zoo_state::animal_behavior::behavior_task_failure_transition;
mod behavior_task_phase_transition_execution;
mod behavior_termination_execution;
mod behavior_token_execution;
mod behavior_trick_learning_execution;
mod behavior_wander_execution;
mod behavior_water_docking_execution;
mod natural_water_targets;
use openzt2_zoo_state::animal_behavior::interaction_container_occupancy;
mod interaction_container_cleanup;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;

use behavior_set_start_request_types::StartBehaviorSet;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorFeedbackDispatched;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFailed;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFinished;

pub struct AnimalBehaviorExecutionPlugin;

impl Plugin for AnimalBehaviorExecutionPlugin {
    fn build(&self, application: &mut App) {
        behavior_object_attachment_execution::register_object_attachment_execution(application);
        behavior_object_spawn_execution::register_object_spawn_execution(application);
        behavior_queue_wait_execution::register_queue_wait_execution(application);
        behavior_hide_execution::register_hide_execution(application);
        behavior_synchronized_set_execution::register_synchronized_behavior_execution(application);
        behavior_wander_execution::register_wander_execution(application);
        behavior_token_execution::register_token_execution(application);
        behavior_water_docking_execution::register(application);
        application.add_systems(
            FixedUpdate,
            (
                behavior_termination_execution::execute_animal_termination_actions,
                behavior_termination_execution::hide_animals_after_authored_removal_period,
            )
                .chain()
                .in_set(FixedGameSet::Think)
                .run_if(in_state(GamePhase::InGame)),
        );
        application
            .init_non_send::<behavior_script_execution::BehaviorScriptContexts>()
            .init_resource::<openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionSequence>()
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
                    .after(openzt2_zoo_state::feeding::ContainerQuantityHydration)
                    .before(behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
                    .in_set(FixedGameSet::Think)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome
                    .in_set(FixedGameSet::Think)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                autonomous_task_selection::task_cooldowns::start_animal_task_cooldowns_after_delayed_tasks_end
                    .after(behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
                    .in_set(FixedGameSet::Think)
                    .run_if(in_state(GamePhase::InGame)),
            );
        application.add_systems(
            FixedUpdate,
            (
                behavior_feedback_dispatch_execution::dispatch_current_authored_behavior_feedback_and_advance,
                interaction_container_cleanup::remove_destroyed_container_memberships,
                behavior_interaction_container_execution::enter_authored_interaction_containers_and_play_selected_behavior_sets,
                (
                    behavior_locomotion_switch_set_execution::play_locomotion_medium_behavior_sets,
                    behavior_locomotion_switch_set_execution::play_animation_node_behavior_sets,
                )
                    .chain(),
                behavior_head_look_execution::continue_after_authored_head_look_steps,
                behavior_docking_execution::start_current_supported_behavior_docking_actions,
                behavior_move_execution::start_current_supported_behavior_move_actions,
                behavior_animation_clip_execution::start_current_behavior_animation_clip_actions,
                behavior_random_animation_execution::start_next_random_behavior_animation,
                behavior_fact_modification::apply_supported_current_behavior_fact_modifications,
                behavior_trick_learning_execution::apply_current_authored_trick_learning,
                behavior_show_event_execution::register_current_authored_animal_show_events,
                behavior_script_execution::execute_authored_behavior_script_actions,
                behavior_clothing_delivery::finish_clothing_delivery,
                behavior_target_test_execution::find_target_and_enter_success_or_failure_behavior_set,
                behavior_target_placement_execution::prepare_and_finish_behavior_target_placements,
                behavior_target_placement_execution::despawn_orphaned_behavior_target_placement_site_markers,
            )
                .chain()
                .after(
                    behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome,
                )
                .after(openzt2_zoo_state::feeding::ContainerQuantityHydration)
                // A stopping walk must see the clip a behavior starts this tick.
                .before(openzt2_zoo_state::locomotion::AnimalAnimationNavigationProjection)
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
                    behavior_animation_clip_execution::finish_behavior_animation_clip_actions_after_play_time,
                ).chain(),
                behavior_locomotion_switch_set_execution::fail_medium_following_sets_after_leaving_dry_ground,
            )
                .in_set(FixedGameSet::Act)
                .run_if(in_state(GamePhase::InGame)),
        );
    }
}
