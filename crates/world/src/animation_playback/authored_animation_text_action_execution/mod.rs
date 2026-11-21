use bevy::prelude::*;
use openzt2_game_data::animation::animation_text_key::AuthoredAnimationTextAction;
use openzt2_world_state::animation_playback::animation_event_message_types::{
    AnimationAiCommandRequested, AnimationDockCommand, AnimationDockCommandRequested,
    AnimationWhapRequested,
};

use crate::animation_playback::authored_animation_control_text_action_execution::set_animation_playback_controller_speed_from_authored_percent;
use crate::animation_playback::authored_animation_text_action_execution_context::AuthoredAnimationTextActionExecutionContext;
use openzt2_world_state::animation_playback::animation_playback_controller_types::{
    AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
};

impl<const REAL_TIME: bool> AuthoredAnimationTextActionExecutionContext<'_, '_, REAL_TIME> {
    pub(super) fn execute_one_crossed_authored_animation_text_command(
        &mut self,
        animation_playback_controller_entity: Entity,
        animation_playback_controller: &mut AnimationPlaybackController,
        event_time_milliseconds: u32,
        authored_animation_text_command: &openzt2_game_assets::animation::animation_set_asset_types::AnimationTimelineCommand,
    ) {
        let resolved_action_target_entity = authored_animation_text_command
            .joint
            .and_then(|joint_key| {
                let rig = self
                    .animation_rig_joints
                    .get(animation_playback_controller_entity)
                    .ok()?;
                self.named_animation_joint_targets
                    .iter_many(rig.joints())
                    .matched()
                    .find_map(|(joint, target)| {
                        (target.authored_joint_id == joint_key).then_some(joint)
                    })
            })
            .unwrap_or(animation_playback_controller_entity);

        match &*authored_animation_text_command.action {
            AuthoredAnimationTextAction::PlaySound { .. }
            | AuthoredAnimationTextAction::RunParticleSystem { .. }
            | AuthoredAnimationTextAction::AttachParticleSystem { .. }
            | AuthoredAnimationTextAction::DetachParticleSystem { .. }
            | AuthoredAnimationTextAction::RunPresentationEffect { .. } => {
                self.presentation_actions.write(super::authored_animation_presentation_action_delivery::AnimationPresentationTextAction::<REAL_TIME> {
                    target: resolved_action_target_entity,
                    milliseconds: event_time_milliseconds,
                    action: authored_animation_text_command.action.clone(),
                });
            }
            AuthoredAnimationTextAction::AttachPendingObject
            | AuthoredAnimationTextAction::SpawnObject
            | AuthoredAnimationTextAction::AttachNamedObject { .. }
            | AuthoredAnimationTextAction::CreateObject { .. }
            | AuthoredAnimationTextAction::CreateObjectOverride { .. }
            | AuthoredAnimationTextAction::DestroyObject { .. }
            | AuthoredAnimationTextAction::KillAnimationSubject
            | AuthoredAnimationTextAction::TriggerSurfaceEffect { .. }
            | AuthoredAnimationTextAction::DetachObject => {
                self.object_commands.write(
                    openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommand {
                        controller: animation_playback_controller_entity,
                        playback_generation: animation_playback_controller.playback_generation,
                        request_id: animation_playback_controller.explicit_clip_request_id,
                        joint: resolved_action_target_entity,
                        action: (*authored_animation_text_command.action).clone(),
                    },
                );
            }
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
            AuthoredAnimationTextAction::ExitAnimation => {
                // Finish at the clip boundary and retain the owning task's completion identity.
                animation_playback_controller.playback_repetition_policy =
                    AnimationPlaybackRepetitionPolicy::PlayOnce;
            }
            AuthoredAnimationTextAction::SendArtificialIntelligenceCommand(command) => {
                self.animation_control_text_action_execution
                    .ai_commands
                    .write(AnimationAiCommandRequested {
                        controller: animation_playback_controller_entity,
                        target: resolved_action_target_entity,
                        command: command.clone(),
                    });
            }
            AuthoredAnimationTextAction::ApplyWhapImpulse {
                impulse_angle_degrees,
                impulse_duration_frames,
            } => {
                self.animation_control_text_action_execution
                    .whap_requests
                    .write(AnimationWhapRequested {
                        controller: animation_playback_controller_entity,
                        target: resolved_action_target_entity,
                        angle_degrees: *impulse_angle_degrees,
                        duration_frames: *impulse_duration_frames,
                    });
            }
            AuthoredAnimationTextAction::SetGroundFit(mode) => {
                let settings = openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationGroundFitMode(*mode);
                let control = &mut self.animation_control_text_action_execution;
                if let Ok(mut current) = control
                    .ground_fit
                    .get_mut(animation_playback_controller_entity)
                {
                    current.set_if_neq(settings);
                } else {
                    control
                        .commands
                        .entity(animation_playback_controller_entity)
                        .try_insert(settings);
                }
            }
            AuthoredAnimationTextAction::SetDockControllerEnabled(enabled) => {
                self.animation_control_text_action_execution
                    .dock_commands
                    .write(AnimationDockCommandRequested {
                        controller: animation_playback_controller_entity,
                        command: AnimationDockCommand::Enable(*enabled),
                    });
            }
            AuthoredAnimationTextAction::GoToDockControllerNode(node) => {
                self.animation_control_text_action_execution
                    .dock_commands
                    .write(AnimationDockCommandRequested {
                        controller: animation_playback_controller_entity,
                        command: AnimationDockCommand::GoToNode(*node),
                    });
            }
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
            AuthoredAnimationTextAction::UnrecognizedAuthoredCommand(_) => {
                unreachable!("asset loader admitted an unlowered animation text-key command")
            }
        }
    }
}
