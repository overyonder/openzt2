use bevy::prelude::*;

use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationGraphNodePlaybackRequest;
use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphEnabled;

use crate::animation_playback::authored_animation_text_action_execution_context::AuthoredAnimationControlTextActionExecutionContext;

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

    pub(super) fn set_authored_animation_graph_enabled(
        &mut self,
        animation_playback_controller_entity: Entity,
        animation_graph_is_enabled: bool,
    ) {
        let enabled = AnimationGraphEnabled(animation_graph_is_enabled);
        if let Ok(mut current) = self
            .graphs_enabled
            .get_mut(animation_playback_controller_entity)
        {
            current.set_if_neq(enabled);
        } else {
            self.commands
                .entity(animation_playback_controller_entity)
                .try_insert(enabled);
        }
    }
}

impl AuthoredAnimationControlTextActionExecutionContext<'_, '_> {
    pub(super) fn configure_authored_traversal_segment_timing(
        &mut self,
        controller: Entity,
        playback_generation: u64,
        completion_percent: i32,
        segment_count: i32,
    ) {
        let Ok(segment_count) = u32::try_from(segment_count) else {
            return;
        };
        if segment_count == 0 {
            return;
        }
        self.traversal_timing.write(
            crate::animation_playback::traversal_animation_path_binding::ConfigureTraversalSegmentTiming {
                controller,
                playback_generation,
                completion_fraction: completion_percent as f32 / 100.0,
                segment_count,
            },
        );
    }
}
