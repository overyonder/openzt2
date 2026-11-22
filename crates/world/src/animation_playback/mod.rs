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
mod authored_binder_rig_binding;
pub(crate) mod authored_decal_texture_cycles;
mod managed_clip_binding;
mod model_joint_attachment_binding;
mod named_model_animation_joint_target_binding;
mod player_clock_lanes;
mod prefab_motion_rotation_projection;
mod procedural_ground_fit;
mod sampled_animation_joint_pose_blending;
mod standard_bevy_animation_player_projection_types;
mod traversal_animation_path_binding;

use crate::world_spawn::prefab_world_instance_spawning::register_prefab_instance_animation;
use bevy::prelude::*;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommandDelivery;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    BehaviorAnimationKeyDelivery, DispatchBehaviorAnimationTextKeys,
};

use self::authored_animation_text_action_execution::execute_behavior_animation_text_key_aliases;

use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;

use crate::animation_playback::animation_traversal_transition_request_projection::project_authored_animation_traversal_transitions_to_graph_node_playback_requests;
use crate::animation_playback::authored_animation_event_execution::execute_crossed_authored_animation_events;
use crate::animation_playback::named_model_animation_joint_target_binding::bind_new_named_model_entities_and_new_animation_playback_controller_descendants;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationCompleted;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommand;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationTraversalTransitionRequested;

#[derive(Default)]
pub struct AnimationPlaybackPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AuthoredAnimationRenderTextKeys;

#[derive(Message)]
pub(crate) struct AnimationNamedPhysicalPresentationCommand<const REAL_TIME: bool>(
    pub(crate) AnimationObjectCommand,
);

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BehaviorAnimationAliasDelivery;

impl Plugin for AnimationPlaybackPlugin {
    fn build(&self, application: &mut App) {
        register_prefab_instance_animation(application);
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
                    .after(bevy::transform::TransformSystems::Propagate)
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
            .add_message::<DispatchBehaviorAnimationTextKeys>()
            .add_message::<AnimationCompleted>()
            .add_message::<openzt2_world_state::animation_playback::animation_event_message_types::AnimationAiCommandRequested>()
            .add_message::<openzt2_world_state::animation_playback::animation_event_message_types::AnimationWhapRequested>()
            .add_message::<openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommand>()
            .add_message::<AnimationTraversalTransitionRequested>()
            .add_systems(
                Update,
                authored_binder_rig_binding::bind_authored_animation_binder_rigs
                    .before(managed_clip_binding::bind_selected_clips)
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                FixedUpdate,
                authored_binder_rig_binding::bind_authored_animation_binder_rigs
                    .before(managed_clip_binding::bind_selected_clips)
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                PostUpdate,
                (
                    procedural_ground_fit::clear_procedural_ground_fit_pose
                        .before(bevy::app::AnimationSystems),
                    procedural_ground_fit::apply_procedural_ground_fit_pose
                        .after(locomotion_root_motion::remove_consumed_root_trajectory)
                        .before(bevy::transform::TransformSystems::Propagate)
                        .run_if(bevy::ecs::schedule::common_conditions::any_with_component::<procedural_ground_fit::GroundFitPose>),
                ),
            )
            .add_systems(
                FixedUpdate,
                authored_decal_texture_cycles::advance_authored_decal_texture_cycles
                    .after(execute_crossed_authored_animation_events::<false>)
                    .in_set(FixedGameSet::Act)
                    .run_if(authored_secondary_motion_enabled)
                    .run_if(bevy::ecs::schedule::common_conditions::any_with_component::<authored_decal_texture_cycles::DecalCyclePlayback>),
            )
            .add_systems(
                FixedUpdate,
                (
                    managed_clip_binding::bind_selected_clips,
                    player_clock_lanes::advance_fixed_players,
                    player_clock_lanes::follow_animation_player_steps::<false>,
                    execute_crossed_authored_animation_events::<false>,
                ).chain()
                    .in_set(openzt2_world_state::animation_playback::locomotion_root_motion::AnimationLocomotionMotionSampling)
                    .before(AnimationObjectCommandDelivery)
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                FixedUpdate,
                (
                    execute_behavior_animation_text_key_aliases
                        .in_set(BehaviorAnimationAliasDelivery)
                        .after(BehaviorAnimationKeyDelivery),
                    authored_animation_presentation_action_delivery::deliver_presentation_text_actions::<false>
                        .after(execute_behavior_animation_text_key_aliases),
                )
                    .in_set(AnimationObjectCommandDelivery)
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                Update,
                (
                    managed_clip_binding::bind_selected_clips,
                    player_clock_lanes::follow_animation_player_steps::<true>,
                    execute_crossed_authored_animation_events::<true>.in_set(AuthoredAnimationRenderTextKeys),
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

fn authored_secondary_motion_enabled(
    graphics: Option<Res<openzt2_game_assets::graphics_settings_types::GraphicsSettings>>,
) -> bool {
    use openzt2_game_assets::graphics_settings_types::ThreeLevelGraphicsDetail;
    graphics.is_none_or(|settings| settings.animation_detail != ThreeLevelGraphicsDetail::Low)
}
