use std::sync::Arc;

use bevy::prelude::*;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationTimelineCommand;
use openzt2_game_data::animation::animation_text_key::AuthoredAnimationTextAction;
use openzt2_world_state::animation_playback::animation_event_message_types::{
    AnimationAiCommandRequested, AnimationObjectCommand, AnimationWhapRequested,
};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::DispatchBehaviorAnimationTextKeys;

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
        authored_animation_text_command: &AnimationTimelineCommand,
    ) {
        self.execute_text_command(
            animation_playback_controller_entity,
            animation_playback_controller,
            event_time_milliseconds,
            animation_playback_controller_entity,
            authored_animation_text_command,
        );
    }

    fn execute_text_command(
        &mut self,
        animation_playback_controller_entity: Entity,
        animation_playback_controller: &mut AnimationPlaybackController,
        event_time_milliseconds: u32,
        default_joint: Entity,
        authored_animation_text_command: &AnimationTimelineCommand,
    ) {
        let action_joint = authored_animation_text_command.joint.and_then(|joint_key| {
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
        });
        if action_joint.is_none()
            && authored_animation_text_command.joint.is_some()
            && matches!(
                &*authored_animation_text_command.action,
                AuthoredAnimationTextAction::CreateObject { .. }
                    | AuthoredAnimationTextAction::CreateObjectOverride { .. }
            )
        {
            return;
        }
        let resolved_action_target_entity = action_joint.unwrap_or(default_joint);

        let object_command = || AnimationObjectCommand {
            controller: animation_playback_controller_entity,
            playback_generation: animation_playback_controller.playback_generation,
            request_id: animation_playback_controller.explicit_clip_request_id,
            joint: resolved_action_target_entity,
            action: (*authored_animation_text_command.action).clone(),
        };
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
            AuthoredAnimationTextAction::CreateObject { .. }
            | AuthoredAnimationTextAction::CreateObjectOverride { .. }
            | AuthoredAnimationTextAction::DestroyObject { .. } => {
                self.binder_commands
                    .write(
                        super::AnimationNamedPhysicalPresentationCommand::<REAL_TIME>(
                            object_command(),
                        ),
                    );
            }
            AuthoredAnimationTextAction::AttachPendingObject
            | AuthoredAnimationTextAction::SpawnObject
            | AuthoredAnimationTextAction::AttachNamedObject { .. }
            | AuthoredAnimationTextAction::KillAnimationSubject
            | AuthoredAnimationTextAction::TriggerSurfaceEffect { .. }
            | AuthoredAnimationTextAction::DetachObject => {
                self.object_commands.write(object_command());
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
                        playback_generation: animation_playback_controller.playback_generation,
                        request_id: animation_playback_controller.explicit_clip_request_id,
                        milliseconds: event_time_milliseconds,
                        target: resolved_action_target_entity,
                        command: command.clone(),
                    });
            }
            AuthoredAnimationTextAction::ApplyWhapImpulse {
                impulse_angle_degrees,
                impulse_random_offset_degrees,
            } => {
                self.animation_control_text_action_execution
                    .whap_requests
                    .write(AnimationWhapRequested {
                        controller: animation_playback_controller_entity,
                        playback_generation: animation_playback_controller.playback_generation,
                        request_id: animation_playback_controller.explicit_clip_request_id,
                        target: resolved_action_target_entity,
                        angle_degrees: *impulse_angle_degrees,
                        random_offset_degrees: *impulse_random_offset_degrees,
                    });
            }
            AuthoredAnimationTextAction::SetGroundFit(mode) => {
                let control = &mut self.animation_control_text_action_execution;
                if let Ok(mut current) = control
                    .ground_fit
                    .get_mut(animation_playback_controller_entity)
                {
                    current.apply_authored_key(*mode);
                }
            }
            AuthoredAnimationTextAction::SetDockControllerEnabled(enabled) => {
                if let Ok(mut cycles) = self
                    .animation_control_text_action_execution
                    .decal_cycles
                    .get_mut(animation_playback_controller_entity)
                {
                    cycles.set_enabled_from_key(*enabled);
                    if cycles.is_advancing() {
                        self.animation_control_text_action_execution
                            .commands
                            .entity(animation_playback_controller_entity)
                            .try_insert_if_new(
                                super::authored_decal_texture_cycles::DecalCyclesAdvancing,
                            );
                    }
                }
            }
            AuthoredAnimationTextAction::GoToDockControllerNode(node) => {
                if let Ok(mut cycles) = self
                    .animation_control_text_action_execution
                    .decal_cycles
                    .get_mut(animation_playback_controller_entity)
                {
                    cycles.set_stop_from_key(*node);
                    if cycles.is_advancing() {
                        self.animation_control_text_action_execution
                            .commands
                            .entity(animation_playback_controller_entity)
                            .try_insert_if_new(
                                super::authored_decal_texture_cycles::DecalCyclesAdvancing,
                            );
                    }
                }
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
                .request_authored_animation_graph_node(
                    animation_playback_controller_entity,
                    animation_graph_node_asset_key,
                ),
            AuthoredAnimationTextAction::UnrecognizedAuthoredCommand(_) => {
                unreachable!("asset loader admitted an unlowered animation text-key command")
            }
        }
    }
}

pub(super) fn execute_behavior_animation_text_key_aliases(
    mut aliases: MessageReader<DispatchBehaviorAnimationTextKeys>,
    mut controllers: Query<&mut AnimationPlaybackController>,
    mut execution: AuthoredAnimationTextActionExecutionContext<false>,
) {
    for alias in aliases.read() {
        let Ok(mut playback) = controllers.get_mut(alias.controller) else {
            continue;
        };
        if playback.playback_generation != alias.playback_generation
            || playback.explicit_clip_request_id != Some(alias.request_id)
        {
            continue;
        }
        for command in &alias.commands {
            let timeline_command = AnimationTimelineCommand {
                joint: command
                    .target_skeleton_joint_asset_key
                    .as_deref()
                    .map(openzt2_game_data::AssetId::from_key),
                action: Arc::new(command.animation_text_action.clone()),
            };
            execution.execute_text_command(
                alias.controller,
                &mut playback,
                alias.milliseconds,
                alias.joint,
                &timeline_command,
            );
        }
    }
}
