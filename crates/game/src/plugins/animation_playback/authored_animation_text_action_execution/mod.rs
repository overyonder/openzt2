use bevy::prelude::*;
use openzt2_game_data::animation::animation_text_key::AuthoredAnimationTextAction;

use super::{
    animation_playback_controller_types::AnimationPlaybackController,
    authored_animation_control_text_action_execution::set_animation_playback_controller_speed_from_authored_percent,
    authored_animation_text_action_execution_context::AuthoredAnimationTextActionExecutionContext,
};

impl AuthoredAnimationTextActionExecutionContext<'_, '_> {
    pub(super) fn execute_one_crossed_authored_animation_text_command(
        &mut self,
        animation_playback_controller_entity: Entity,
        animation_playback_controller: &mut AnimationPlaybackController,
        event_time_milliseconds: u32,
        authored_animation_text_command: &openzt2_game_data::animation::animation_text_key::AuthoredAnimationTextCommand,
    ) {
        let authored_target_joint_asset_key = authored_animation_text_command
            .target_skeleton_joint_asset_key
            .as_deref()
            .unwrap_or("");
        let resolved_action_target_entity = self
            .named_animation_joint_targets
            .iter()
            .find_map(|(joint_entity, joint_target)| {
                (joint_target.animation_playback_controller_entity
                    == animation_playback_controller_entity
                    && joint_target.joint_asset_key == authored_target_joint_asset_key)
                    .then_some(joint_entity)
            })
            .unwrap_or(animation_playback_controller_entity);

        match &authored_animation_text_command.animation_text_action {
            AuthoredAnimationTextAction::PlaySound {
                audio_asset_key,
                playback_is_looped,
                update_sound_position_during_playback,
                ..
            } => self
                .audio_and_effect_text_action_execution
                .play_authored_animation_sound(
                    animation_playback_controller_entity,
                    event_time_milliseconds,
                    audio_asset_key,
                    *playback_is_looped,
                    *update_sound_position_during_playback,
                ),
            AuthoredAnimationTextAction::AttachPendingObject
            | AuthoredAnimationTextAction::AttachNamedObject { .. }
            | AuthoredAnimationTextAction::DetachObject => {
                self.object_commands.write(
                    super::animation_event_message_types::AnimationObjectCommand {
                        controller: animation_playback_controller_entity,
                        playback_generation: animation_playback_controller.playback_generation,
                        request_id: animation_playback_controller.explicit_clip_request_id,
                        joint: resolved_action_target_entity,
                        action: authored_animation_text_command
                            .animation_text_action
                            .clone(),
                    },
                );
            }
            AuthoredAnimationTextAction::RunParticleSystem {
                particle_system_asset_key,
                particle_scale,
                ..
            } => self
                .audio_and_effect_text_action_execution
                .run_authored_animation_particle_system(
                    resolved_action_target_entity,
                    particle_system_asset_key,
                    particle_scale.unwrap_or(1.0),
                ),
            AuthoredAnimationTextAction::AttachParticleSystem {
                particle_system_asset_key,
                ..
            } => self
                .audio_and_effect_text_action_execution
                .attach_authored_animation_particle_system(
                    resolved_action_target_entity,
                    particle_system_asset_key,
                ),
            AuthoredAnimationTextAction::RunPresentationEffect {
                presentation_effect_asset_key,
                attach_to_target,
                stop_attached_effects: false,
                water_height_offset,
                ..
            } => self
                .audio_and_effect_text_action_execution
                .run_authored_animation_presentation_effect(
                    resolved_action_target_entity,
                    presentation_effect_asset_key,
                    *attach_to_target,
                    water_height_offset.unwrap_or(0.0),
                ),
            AuthoredAnimationTextAction::DetachParticleSystem { .. }
            | AuthoredAnimationTextAction::RunPresentationEffect {
                stop_attached_effects: true,
                ..
            } => self
                .audio_and_effect_text_action_execution
                .stop_authored_animation_effects_attached_to_target(resolved_action_target_entity),
            AuthoredAnimationTextAction::PlayAnimationGraphNode {
                animation_graph_node_asset_key,
                blend_duration_seconds,
                advance_current_animation_time,
                enter_immediately,
            } => self
                .animation_control_text_action_execution
                .request_authored_animation_graph_node_with_playback_policy(
                    animation_playback_controller_entity,
                    animation_graph_node_asset_key,
                    blend_duration_seconds.map_or(0, |seconds| (seconds.max(0.0) * 1000.0) as u32),
                    *advance_current_animation_time,
                    *enter_immediately,
                ),
            AuthoredAnimationTextAction::RequestAnimationGraphNode(
                animation_graph_node_asset_key,
            ) => self
                .animation_control_text_action_execution
                .request_authored_animation_graph_node(
                    animation_playback_controller_entity,
                    animation_graph_node_asset_key,
                ),
            AuthoredAnimationTextAction::StartAlignedAnimation {
                animation_graph_node_asset_key,
                playback_is_looped,
                ..
            } => self
                .animation_control_text_action_execution
                .start_authored_animation_with_looping_policy(
                    animation_playback_controller_entity,
                    animation_playback_controller,
                    animation_graph_node_asset_key,
                    *playback_is_looped,
                ),
            AuthoredAnimationTextAction::SetAnimationGraphEnabled(enabled) => self
                .animation_control_text_action_execution
                .set_authored_animation_graph_enabled(
                    animation_playback_controller_entity,
                    *enabled,
                ),
            AuthoredAnimationTextAction::ExitAnimation
            | AuthoredAnimationTextAction::SendArtificialIntelligenceCommand(_)
            | AuthoredAnimationTextAction::ApplyWhapImpulse { .. }
            | AuthoredAnimationTextAction::TriggerSurfaceEffect { .. }
            | AuthoredAnimationTextAction::SetGroundFit(_)
            | AuthoredAnimationTextAction::SetDockControllerEnabled(_)
            | AuthoredAnimationTextAction::GoToDockControllerNode(_) => {}
            AuthoredAnimationTextAction::SetAnimationPlaybackSpeed {
                playback_speed_percent,
                ..
            } => {
                set_animation_playback_controller_speed_from_authored_percent(
                    animation_playback_controller,
                    *playback_speed_percent,
                );
            }
            AuthoredAnimationTextAction::RequestTraversalTransition {
                animation_graph_node_asset_key,
            } => self
                .animation_control_text_action_execution
                .request_authored_animation_traversal_transition(
                    animation_playback_controller_entity,
                    animation_graph_node_asset_key,
                ),
            AuthoredAnimationTextAction::SpawnObject
            | AuthoredAnimationTextAction::CreateObject { .. }
            | AuthoredAnimationTextAction::CreateObjectOverride { .. }
            | AuthoredAnimationTextAction::DestroyObject { .. }
            | AuthoredAnimationTextAction::KillAnimationSubject => {}
            AuthoredAnimationTextAction::UnrecognizedAuthoredCommand(_) => {
                unreachable!("asset loader admitted an unlowered animation text-key command")
            }
        }
    }
}
