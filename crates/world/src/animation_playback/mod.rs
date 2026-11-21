mod locomotion_root_motion;
// Runtime animation playback, authored events, attachments, and Bevy player projection.

mod animation_playback_controller_ancestor_lookup;
mod animation_traversal_transition_request_projection;
mod animation_water_effect_binding;
mod authored_animation_audio_and_effect_text_action_execution;
mod authored_animation_control_text_action_execution;
mod authored_animation_event_execution;
mod authored_animation_presentation_action_delivery;
mod authored_animation_sound_resolution;
mod authored_animation_text_action_execution;
mod authored_animation_text_action_execution_context;
mod managed_clip_binding;
mod model_joint_attachment_binding;
mod named_model_animation_joint_target_binding;
mod player_clock_lanes;
mod prefab_motion_rotation_projection;
mod sampled_animation_joint_pose_blending;
mod standard_bevy_animation_player_projection_types;
mod traversal_animation_path_binding;

use crate::world_spawn::prefab_world_instance_spawning::morph_fade_projection;
use bevy::prelude::*;

use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;

use crate::animation_playback::animation_traversal_transition_request_projection::project_authored_animation_traversal_transitions_to_graph_node_playback_requests;
use crate::animation_playback::authored_animation_event_execution::execute_crossed_authored_animation_events;
use crate::animation_playback::named_model_animation_joint_target_binding::bind_new_named_model_entities_and_new_animation_playback_controller_descendants;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationCompleted;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationTraversalTransitionRequested;

#[derive(Default)]
pub struct AnimationPlaybackPlugin;

impl Plugin for AnimationPlaybackPlugin {
    fn build(&self, application: &mut App) {
        if application.is_plugin_added::<AnimationPlugin>() {
            application
                .remove_systems_in_set(
                    PostUpdate,
                    bevy::animation::advance_animations,
                    bevy::ecs::schedule::ScheduleCleanupPolicy::RemoveSystemsOnly,
                )
                .expect("stock animation advancement is registered");
        }
        application
            .add_systems(FixedUpdate, morph_fade_projection::advance_morph_fade_timers.in_set(FixedGameSet::Act))
            .add_systems(Update, morph_fade_projection::project_morph_fade_onto_prefab_materials.in_set(GameSet::Presentation))
            .add_message::<authored_animation_presentation_action_delivery::AnimationPresentationTextAction<false>>()
            .add_message::<authored_animation_presentation_action_delivery::AnimationPresentationTextAction<true>>()
            .add_message::<player_clock_lanes::AnimationPlaybackInterval<false>>()
            .add_message::<player_clock_lanes::AnimationPlaybackInterval<true>>()
            .add_systems(PostUpdate, player_clock_lanes::advance_render_players
                .after(bevy::animation::transition::advance_transitions)
                .before(bevy::animation::animate_targets)
                .in_set(bevy::app::AnimationSystems))
            .add_systems(PostUpdate, managed_clip_binding::retire_unbound_players.before(bevy::app::AnimationSystems))
            .add_systems(PostUpdate,
                animation_water_effect_binding::update_animation_water_effect_planes
                    .after(bevy::app::AnimationSystems)
                    .before(openzt2_game_assets::effect::particle_plane_binding::ParticlePlaneSources))
            .add_systems(PostUpdate,
                prefab_motion_rotation_projection::project_prefab_motion_rotations
                    .after(bevy::app::AnimationSystems)
                    .before(openzt2_world_state::world_spawn::prefab_authored_billboard_orientation_mode::PrefabBillboardProjection)
                    .before(bevy::transform::TransformSystems::Propagate))
            .add_systems(PostUpdate,
                (sampled_animation_joint_pose_blending::blend_sampled_animation_joint_poses,
                crate::animation_playback::locomotion_root_motion::remove_consumed_root_trajectory).chain()
                    .after(bevy::app::AnimationSystems)
                    .before(bevy::transform::TransformSystems::Propagate))
            .add_systems(PostUpdate,
                bind_new_named_model_entities_and_new_animation_playback_controller_descendants
                    .before(bevy::app::AnimationSystems))
            .add_message::<AnimationCompleted>()
            .add_message::<openzt2_world_state::animation_playback::animation_event_message_types::AnimationAiCommandRequested>()
            .add_message::<openzt2_world_state::animation_playback::animation_event_message_types::AnimationDockCommandRequested>()
            .add_message::<openzt2_world_state::animation_playback::animation_event_message_types::AnimationWhapRequested>()
            .add_message::<openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommand>()
            .add_message::<AnimationTraversalTransitionRequested>()
            .add_systems(
                FixedUpdate,
                (
                    managed_clip_binding::bind_selected_clips,
                    player_clock_lanes::advance_fixed_players,
                    player_clock_lanes::follow_animation_player_steps::<false>,
                    execute_crossed_authored_animation_events::<false>,
                    authored_animation_presentation_action_delivery::deliver_presentation_text_actions::<false>,
                ).chain()
                    .in_set(openzt2_world_state::animation_playback::locomotion_root_motion::AnimationLocomotionMotionSampling)
                    .before(openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommandDelivery)
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                Update,
                (
                    managed_clip_binding::bind_selected_clips,
                    player_clock_lanes::follow_animation_player_steps::<true>,
                    execute_crossed_authored_animation_events::<true>,
                )
                    .chain()
                    .in_set(GameSet::Presentation),
            )
            .add_systems(Update, (
                model_joint_attachment_binding::bind_model_joint_attachments,
                traversal_animation_path_binding::bind_authored_traversal_animation_paths,
                authored_animation_presentation_action_delivery::deliver_presentation_text_actions::<true>
                    .after(execute_crossed_authored_animation_events::<true>),
                project_authored_animation_traversal_transitions_to_graph_node_playback_requests
                    .after(execute_crossed_authored_animation_events::<true>),
            ).in_set(GameSet::Presentation));
    }
}
