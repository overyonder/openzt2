//! Live execution of typed per-document behavior declarations.

mod authored_disease_exposure;
pub(crate) mod autonomous_task_selection;
mod behavior_follow_entity_execution;
mod behavior_special_docking;
mod behavior_splash_guest_execution;
mod behavior_staff_training_execution;

/// Whether an escaped animal can reach a point: outside every habitat, or in
/// one whose fence is breached, rather than behind another habitat's intact
/// fence.
pub(super) fn escaped_animal_reaches(
    habitats: &openzt2_zoo_state::habitat::habitat_types::HabitatIndex,
    grid: openzt2_zoo_state::topology::topology_graph_types::TopologyGrid,
    summaries: &Query<&openzt2_zoo_state::habitat::habitat_types::HabitatSummary>,
    point: Vec2,
) -> bool {
    openzt2_zoo_state::habitat::habitat_membership_and_containment::locate_habitat_at_world_position(
        habitats, point, grid,
    )
    .is_none_or(|habitat| {
        summaries
            .get(habitat)
            .is_ok_and(|summary| summary.boundary_is_breached)
    })
}
mod behavior_animation_clip_execution;
mod behavior_animation_key_execution;
use openzt2_zoo_state::animal_behavior::behavior_candidate_selection;
mod behavior_clothing_delivery;
mod behavior_container_admission;
mod behavior_docking_execution;
use openzt2_zoo_state::animal_behavior::behavior_entity_role_resolution;
mod behavior_face_target_execution;
mod behavior_fact_modification;
mod behavior_feedback_dispatch_execution;
pub(crate) mod behavior_fence_crossing_execution;
mod behavior_head_look_execution;
mod behavior_hide_execution;
mod behavior_interaction_container_execution;
mod behavior_locomotion_switch_set_execution;
pub(crate) mod behavior_move_execution;
mod behavior_object_attachment_execution;
mod behavior_object_spawn_execution;
mod behavior_pursuit_execution;
mod behavior_queue_wait_execution;
mod behavior_random_animation_execution;
mod behavior_random_choice_execution;
mod behavior_random_stream_initialization;
mod behavior_target_morph_execution;
mod placed_object_virtual_nodes;
use openzt2_zoo_state::animal_behavior::behavior_random_stream_state;
mod behavior_script_execution;
mod behavior_set_start_request_handling;
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types;
mod behavior_show_event_execution;
mod behavior_spatial_candidates;
mod behavior_subject_type_resolution;
pub(crate) mod behavior_synchronized_set_execution;
pub(crate) mod behavior_target_placement_execution;
use openzt2_zoo_state::animal_behavior::behavior_target_placement_types;
mod behavior_target_test_execution;
use openzt2_zoo_state::behavior_task_execution_types::running_actions as behavior_task_failure_transition;
mod behavior_task_phase_transition_execution;
mod behavior_termination_execution;
mod behavior_token_execution;
mod behavior_traversal_execution;
mod behavior_trick_learning_execution;
mod behavior_trigger_execution;
mod behavior_wander_execution;
mod behavior_water_docking_execution;
mod natural_water_targets;

use bevy::prelude::*;

use behavior_set_start_request_types::StartBehaviorSet;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    activate_behavior_failure, activate_due_behavior_instructions, BehaviorStepFixedSet,
};
use openzt2_zoo_state::behavior_task_execution_types::BehaviorFeedbackDispatched;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFailed;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFinished;

pub struct AnimalBehaviorExecutionPlugin;

impl Plugin for AnimalBehaviorExecutionPlugin {
    fn build(&self, application: &mut App) {
        use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
            activated_behavior_actor, update_waiting_behavior_actors,
        };
        application.add_observer(activated_behavior_actor::<openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorPlaceTarget>.pipe(behavior_target_placement_execution::prepare_and_finish_behavior_target_placements));
        application.add_observer(
            activated_behavior_actor::<
                openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorMove,
            >
                .pipe(behavior_move_execution::start_current_supported_behavior_move_actions),
        );
        application.add_observer(activated_behavior_actor::<(openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorDock, openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorDockQueue)>.pipe(behavior_docking_execution::start_current_supported_behavior_docking_actions));
        application.add_observer(activated_behavior_actor::<openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorAnimationClip>.pipe(behavior_animation_clip_execution::start_current_behavior_animation_clip_actions));
        application.add_observer(activated_behavior_actor::<openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorRandomAnimation>.pipe(behavior_random_animation_execution::start_next_random_behavior_animation));

        application
            .configure_sets(FixedUpdate, BehaviorStepFixedSet::Think
                .in_set(FixedGameSet::Think)
                .after(autonomous_task_selection::start_selected_idle_animal_tasks)
                .after(behavior_task_phase_transition_execution::advance_behavior_target_retry_clocks)
                .after(openzt2_zoo_state::feeding::ContainerQuantityHydration)
                .before(openzt2_zoo_state::locomotion::AnimalAnimationNavigationProjection)
                .run_if(in_state(GamePhase::InGame)))
            .configure_sets(FixedUpdate, BehaviorStepFixedSet::Act
                .in_set(FixedGameSet::Act)
                .run_if(in_state(GamePhase::InGame)))
            .add_observer(activate_behavior_failure)
            .add_observer(authored_disease_exposure::add_authored_disease_target)
            .add_observer(fail_invalid_authored_instruction)
            .add_observer(behavior_feedback_dispatch_execution::dispatch_current_authored_behavior_feedback_and_advance)
            .add_observer(behavior_head_look_execution::continue_after_authored_head_look_steps)
            .add_observer(behavior_interaction_container_execution::enter_authored_interaction_containers_and_play_selected_behavior_sets)
            .add_observer(behavior_locomotion_switch_set_execution::play_locomotion_medium_behavior_sets)
            .add_observer(behavior_locomotion_switch_set_execution::play_animation_node_behavior_sets)
            .add_observer(behavior_fact_modification::apply_supported_current_behavior_fact_modifications)
            .add_observer(behavior_trick_learning_execution::apply_current_authored_trick_learning)
            .add_observer(behavior_show_event_execution::register_current_authored_animal_show_events)
            .add_observer(behavior_script_execution::execute_authored_behavior_script_actions)
            .add_observer(behavior_target_test_execution::find_target_and_enter_success_or_failure_behavior_set)
            .add_observer(behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
            .add_systems(FixedUpdate, activate_due_behavior_instructions.in_set(BehaviorStepFixedSet::Think));
        behavior_special_docking::register(application);
        behavior_follow_entity_execution::register(application);
        behavior_splash_guest_execution::register(application);
        behavior_staff_training_execution::register(application);
        behavior_animation_key_execution::register(application);
        behavior_object_attachment_execution::register_object_attachment_execution(application);
        behavior_object_spawn_execution::register_object_spawn_execution(application);
        behavior_queue_wait_execution::register_queue_wait_execution(application);
        behavior_hide_execution::register_hide_execution(application);
        behavior_synchronized_set_execution::register_synchronized_behavior_execution(application);
        behavior_spatial_candidates::register_behavior_spatial_candidates(application);
        behavior_wander_execution::register_wander_execution(application);
        behavior_pursuit_execution::register_pursuit_execution(application);
        behavior_face_target_execution::register_face_target_execution(application);
        behavior_traversal_execution::register_behavior_traversal_execution(application);
        behavior_target_morph_execution::register_target_morph_execution(application);
        behavior_fence_crossing_execution::register_fence_crossing_execution(application);
        placed_object_virtual_nodes::register_placed_object_virtual_nodes(application);
        behavior_token_execution::register_token_execution(application);
        behavior_trigger_execution::register_trigger_execution(application);
        behavior_water_docking_execution::register(application);
        natural_water_targets::register_water_target_projection(application);
        autonomous_task_selection::target_types::register_target_type_projection(application);
        application.add_systems(
            FixedUpdate,
            authored_disease_exposure::expose_animals_to_authored_diseases
                .in_set(FixedGameSet::Think)
                .after(crate::animal_lifecycle::ResolveAnimalLifeStages)
                .before(autonomous_task_selection::select_idle_animal_tasks)
                .run_if(in_state(GamePhase::InGame))
                .run_if(openzt2_world_state::simulation_time::simulation_control_queries::simulation_is_running)
                .run_if(any_with_component::<ZooSeed>)
                .run_if(authored_disease_exposure::disease_exposure_is_due),
        );
        application
            .add_observer(authored_disease_exposure::clear_authored_disease_tokens_when_cured);
        application
            .add_observer(behavior_termination_execution::execute_animal_termination_actions)
            .add_systems(
                FixedUpdate,
                behavior_termination_execution::hide_animals_after_authored_removal_period
                    .in_set(FixedGameSet::Act)
                    .run_if(in_state(GamePhase::InGame)),
            );
        application
            .init_non_send::<behavior_script_execution::BehaviorScriptContexts>()
            .add_observer(behavior_interaction_container_execution::apply_interaction_container_entry)
            .add_observer(behavior_docking_execution::request_dock_after_container_admission)
            .add_observer(openzt2_zoo_state::behavior_task_execution_types::running_actions::apply_behavior_instruction_completion)
            .add_observer(openzt2_zoo_state::behavior_task_execution_types::running_actions::resume_suspended_behavior_instruction)
            .add_observer(openzt2_zoo_state::behavior_task_execution_types::running_actions::reset_replaced_behavior_task)
            .add_message::<StartBehaviorSet>()
            .add_message::<BehaviorFeedbackDispatched>()
            .add_message::<BehaviorTaskFailed>()
            .add_message::<BehaviorTaskFinished>()
            .add_message::<openzt2_zoo_state::animal_behavior::behavior_birth_notice_types::BehaviorBirthNotice>()
            .add_systems(
                FixedUpdate,
                openzt2_zoo_state::behavior_task_execution_types::running_actions::refresh_behavior_instructions_after_document_changes
                    .in_set(FixedGameSet::Clock)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                behavior_target_placement_execution::invalidate_obsolete_behavior_target_placement_sites
                    .in_set(FixedGameSet::Clock)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    openzt2_zoo_state::animal_behavior::behavior_target_retry_delays::reap_cleared_behavior_target_retries,
                    behavior_task_phase_transition_execution::begin_behavior_target_candidate_selection_updates,
                ).chain()
                    .in_set(FixedGameSet::Clock)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                behavior_random_stream_initialization::attach_behavior_random_stream_to_animals_and_staff
                    .in_set(FixedGameSet::Clock)
                    .run_if(in_state(GamePhase::InGame))
                    .run_if(any_with_component::<ZooSeed>),
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
                    .after(crate::animal_lifecycle::ResolveAnimalLifeStages)
                    .in_set(FixedGameSet::Think)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                autonomous_task_selection::start_selected_idle_animal_tasks
                    .after(autonomous_task_selection::select_idle_animal_tasks)
                    .in_set(FixedGameSet::Think)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    behavior_task_phase_transition_execution::advance_behavior_target_retry_clocks,
                    autonomous_task_selection::task_cooldowns::start_animal_task_cooldowns_after_delayed_tasks_end
                        .after(BehaviorStepFixedSet::Think),
                )
                    .chain()
                    .after(autonomous_task_selection::start_selected_idle_animal_tasks)
                    .after(openzt2_zoo_state::feeding::ContainerQuantityHydration)
                    .before(openzt2_zoo_state::locomotion::AnimalAnimationNavigationProjection)
                    .in_set(FixedGameSet::Think)
                    .run_if(in_state(GamePhase::InGame)),
            );
        application.add_systems(
            FixedUpdate,
            (
                update_waiting_behavior_actors.pipe(behavior_docking_execution::start_current_supported_behavior_docking_actions),
                update_waiting_behavior_actors.pipe(behavior_move_execution::start_current_supported_behavior_move_actions),
                update_waiting_behavior_actors.pipe(behavior_animation_clip_execution::start_current_behavior_animation_clip_actions),
                update_waiting_behavior_actors.pipe(behavior_random_animation_execution::start_next_random_behavior_animation),
                behavior_script_execution::remove_settled_behavior_script_transactions,
                behavior_clothing_delivery::finish_clothing_delivery,
                update_waiting_behavior_actors.pipe(behavior_target_placement_execution::prepare_and_finish_behavior_target_placements),
            ).in_set(BehaviorStepFixedSet::Think),
        );
        application.add_systems(
            FixedUpdate,
            (
                (
                    behavior_docking_execution::fail_behavior_docking_actions_after_navigation_failure,
                    behavior_docking_execution::finish_behavior_docking_actions_after_matching_arrival
                        .after(openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommandDelivery),
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
                behavior_locomotion_switch_set_execution::switch_medium_following_sets_after_medium_changes,
            )
                .in_set(BehaviorStepFixedSet::Act)
                .run_if(in_state(GamePhase::InGame)),
        );
    }
}

fn fail_invalid_authored_instruction(
    activation: On<openzt2_zoo_state::behavior_task_execution_types::running_actions::WhenBehaviorAction<openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorInvalidAuthoredAction>>,
    invalid: Query<&openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorInvalidAuthoredAction>,
    mut commands: Commands,
) {
    if let Ok(action) = invalid.get(activation.entity) {
        warn!(actor = ?activation.entity, action = %action.0, "invalid authored behavior instruction");
        behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
            activation.entity,
            &mut commands,
        );
    }
}

#[cfg(test)]
mod escaped_reach_tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use openzt2_zoo_state::habitat::habitat_types::{HabitatIndex, HabitatSummary};
    use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

    #[test]
    fn an_escaped_animal_reaches_open_ground_and_breached_pens_but_not_closed_ones() {
        let mut world = World::new();
        let summary = |breached| HabitatSummary {
            land_area_square_metres: 1.0,
            water_area_square_metres: 0.0,
            biome_areas_square_metres: Box::new([]),
            boundary_is_breached: breached,
        };
        let breached = world.spawn(summary(true)).id();
        let closed = world.spawn(summary(false)).id();
        let mut index = HabitatIndex::default();
        index.record_habitat_for_topology_cell(IVec2::new(0, 0), breached);
        index.record_habitat_for_topology_cell(IVec2::new(4, 0), closed);
        world.spawn((
            index,
            TopologyGrid {
                origin: Vec2::ZERO,
                spacing_m: 1.0,
            },
        ));
        let reaches = world
            .run_system_once(
                |index: Single<&HabitatIndex>,
                 grid: Single<&TopologyGrid>,
                 summaries: Query<&HabitatSummary>| {
                    [
                        Vec2::new(0.5, 0.5),
                        Vec2::new(4.5, 0.5),
                        Vec2::new(9.5, 0.5),
                    ]
                    .map(|point| super::escaped_animal_reaches(&index, **grid, &summaries, point))
                },
            )
            .unwrap();
        // Its own breached pen, another closed pen, open ground.
        assert_eq!(reaches, [true, false, true]);
    }
}
