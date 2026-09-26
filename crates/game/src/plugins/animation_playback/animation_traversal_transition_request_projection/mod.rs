use bevy::prelude::*;

use crate::plugins::animation_graph::animation_graph_playback_message_types::AnimationGraphNodePlaybackRequest;

use super::animation_event_message_types::AnimationTraversalTransitionRequested;

pub(super) fn project_authored_animation_traversal_transitions_to_graph_node_playback_requests(
    mut traversal_transition_requests: MessageReader<AnimationTraversalTransitionRequested>,
    mut animation_graph_node_requests: MessageWriter<AnimationGraphNodePlaybackRequest>,
) {
    for traversal_transition_request in traversal_transition_requests.read() {
        animation_graph_node_requests.write(AnimationGraphNodePlaybackRequest::new(
            traversal_transition_request.animation_playback_controller_entity,
            traversal_transition_request
                .animation_graph_node_asset_key
                .clone(),
        ));
    }
}
