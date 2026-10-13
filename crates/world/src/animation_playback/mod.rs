mod locomotion_root_motion;
// Runtime animation playback, authored events, attachments, and Bevy player projection.

mod animation_playback_controller_ancestor_lookup;
mod animation_playback_time_advancement;
mod animation_traversal_transition_request_projection;
mod authored_animation_audio_and_effect_text_action_execution;
mod authored_animation_control_text_action_execution;
mod authored_animation_event_cursor_types;
mod authored_animation_event_execution;
mod authored_animation_text_action_execution;
mod authored_animation_text_action_execution_context;
mod model_joint_attachment_binding;
mod named_model_animation_joint_target_binding;
mod standard_bevy_animation_graph_cache_invalidation;
mod standard_bevy_animation_player_projection;
mod standard_bevy_animation_player_projection_types;

use bevy::prelude::*;

use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;

use crate::animation_playback::animation_playback_time_advancement::advance_real_time_clock_animation_playback_controllers;
use crate::animation_playback::animation_playback_time_advancement::advance_simulation_clock_animation_playback_controllers;
use crate::animation_playback::animation_traversal_transition_request_projection::project_authored_animation_traversal_transitions_to_graph_node_playback_requests;
use crate::animation_playback::authored_animation_event_execution::execute_crossed_authored_animation_events;
use crate::animation_playback::authored_animation_event_execution::initialize_authored_animation_event_cursors_for_new_playback_controllers;
use crate::animation_playback::named_model_animation_joint_target_binding::bind_new_named_model_entities_and_new_animation_playback_controller_descendants;
use crate::animation_playback::standard_bevy_animation_graph_cache_invalidation::invalidate_standard_bevy_animation_graphs_and_player_bindings_after_asset_changes;
use crate::animation_playback::standard_bevy_animation_player_projection::project_animation_playback_controller_state_to_standard_bevy_animation_players;
use crate::animation_playback::standard_bevy_animation_player_projection_types::StandardBevyAnimationGraphsByAnimationSetAsset;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationCompleted;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationTraversalTransitionRequested;

#[derive(Default)]
pub struct AnimationPlaybackPlugin;

impl Plugin for AnimationPlaybackPlugin {
    fn build(&self, application: &mut App) {
        application
            .add_systems(PostUpdate,
                crate::animation_playback::locomotion_root_motion::remove_consumed_root_trajectory
                    .after(bevy::app::AnimationSystems)
                    .before(bevy::transform::TransformSystems::Propagate))
            .init_resource::<StandardBevyAnimationGraphsByAnimationSetAsset>()
            .add_message::<AnimationCompleted>()
            .add_message::<openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommand>()
            .add_message::<AnimationTraversalTransitionRequested>()
            .add_systems(
                FixedUpdate,
                (
                    initialize_authored_animation_event_cursors_for_new_playback_controllers,
                    advance_simulation_clock_animation_playback_controllers,
                    execute_crossed_authored_animation_events,
                ).chain()
                    .before(openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommandDelivery)
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                Update,
                (
                    advance_real_time_clock_animation_playback_controllers,
                    initialize_authored_animation_event_cursors_for_new_playback_controllers,
                    execute_crossed_authored_animation_events,
                    project_authored_animation_traversal_transitions_to_graph_node_playback_requests,
                    invalidate_standard_bevy_animation_graphs_and_player_bindings_after_asset_changes,
                    bind_new_named_model_entities_and_new_animation_playback_controller_descendants,
                    crate::animation_playback::model_joint_attachment_binding::bind_model_joint_attachments,
                    project_animation_playback_controller_state_to_standard_bevy_animation_players,
                )
                    .chain()
                    .in_set(GameSet::Presentation),
            );
    }
}
