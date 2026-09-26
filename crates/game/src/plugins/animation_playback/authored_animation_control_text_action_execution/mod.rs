use bevy::prelude::*;

use crate::plugins::animation_graph::{
    animation_graph_playback_message_types::AnimationGraphNodePlaybackRequest,
    animation_graph_playback_state_types::AnimationGraphEnabled,
};

use super::{
    animation_event_message_types::AnimationTraversalTransitionRequested,
    animation_playback_controller_types::{
        AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
    },
    authored_animation_text_action_execution_context::AuthoredAnimationControlTextActionExecutionContext,
};

impl AuthoredAnimationControlTextActionExecutionContext<'_, '_> {
    pub(super) fn request_authored_animation_graph_node(
        &mut self,
        animation_playback_controller_entity: Entity,
        animation_graph_node_asset_key: &str,
    ) {
        self.animation_node_messages
            .write(AnimationGraphNodePlaybackRequest::new(
                animation_playback_controller_entity,
                animation_graph_node_asset_key.to_owned(),
            ));
    }

    pub(super) fn request_authored_animation_graph_node_with_playback_policy(
        &mut self,
        animation_playback_controller_entity: Entity,
        animation_graph_node_asset_key: &str,
        blend_duration_milliseconds: u32,
        preserve_current_time: bool,
        enter_immediately: bool,
    ) {
        self.animation_node_messages.write(
            AnimationGraphNodePlaybackRequest::with_authored_transition_and_time_policy(
                animation_playback_controller_entity,
                animation_graph_node_asset_key.to_owned(),
                blend_duration_milliseconds,
                preserve_current_time,
                enter_immediately,
            ),
        );
    }

    pub(super) fn start_authored_animation_with_looping_policy(
        &mut self,
        animation_playback_controller_entity: Entity,
        animation_playback_controller: &mut AnimationPlaybackController,
        animation_graph_node_asset_key: &str,
        playback_loops: bool,
    ) {
        animation_playback_controller.playback_repetition_policy = if playback_loops {
            AnimationPlaybackRepetitionPolicy::Loop
        } else {
            AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy
        };
        self.request_authored_animation_graph_node(
            animation_playback_controller_entity,
            animation_graph_node_asset_key,
        );
    }

    pub(super) fn set_authored_animation_graph_enabled(
        &mut self,
        animation_playback_controller_entity: Entity,
        animation_graph_is_enabled: bool,
    ) {
        self.commands
            .entity(animation_playback_controller_entity)
            .insert(AnimationGraphEnabled(animation_graph_is_enabled));
    }

    pub(super) fn request_authored_animation_traversal_transition(
        &mut self,
        animation_playback_controller_entity: Entity,
        animation_graph_node_asset_key: &str,
    ) {
        self.traversal_messages
            .write(AnimationTraversalTransitionRequested {
                animation_playback_controller_entity,
                animation_graph_node_asset_key: animation_graph_node_asset_key.to_owned(),
            });
    }
}

pub(super) fn set_animation_playback_controller_speed_from_authored_percent(
    animation_playback_controller: &mut AnimationPlaybackController,
    playback_speed_percent: i32,
) {
    animation_playback_controller.playback_speed_permille = playback_speed_percent
        .saturating_mul(10)
        .clamp(i32::from(i16::MIN), i32::from(i16::MAX))
        as i16;
}
