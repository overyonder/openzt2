use bevy::prelude::*;

use crate::plugins::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;

/// Typed gameplay or presentation intent to enter a compiled graph node.
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub(crate) struct AnimationGraphNodePlaybackRequest {
    pub(crate) animation_subject_entity: Entity,
    pub(crate) animation_graph_node_asset_key: String,
    pub(crate) blend_duration_milliseconds: Option<u32>,
    pub(crate) preserves_current_playback_time: bool,
    pub(crate) enters_immediately: bool,
}

impl AnimationGraphNodePlaybackRequest {
    pub(crate) fn new(
        animation_subject_entity: Entity,
        animation_graph_node_asset_key: String,
    ) -> Self {
        Self {
            animation_subject_entity,
            animation_graph_node_asset_key,
            blend_duration_milliseconds: None,
            preserves_current_playback_time: false,
            enters_immediately: false,
        }
    }

    pub(crate) fn with_authored_transition_and_time_policy(
        animation_subject_entity: Entity,
        animation_graph_node_asset_key: String,
        blend_duration_milliseconds: u32,
        preserves_current_playback_time: bool,
        enters_immediately: bool,
    ) -> Self {
        Self {
            animation_subject_entity,
            animation_graph_node_asset_key,
            blend_duration_milliseconds: Some(blend_duration_milliseconds),
            preserves_current_playback_time,
            enters_immediately,
        }
    }
}

/// Typed request for one explicit authored clip. Traversal-path BFMs expose
/// several clips through one graph node, so this request names the clip itself.
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub(crate) struct AnimationClipPlaybackRequest {
    pub(crate) animation_subject_entity: Entity,
    pub(crate) animation_clip_asset_key: String,
    pub(crate) blend_duration_milliseconds: u32,
    pub(crate) playback_speed_permille: i16,
    pub(crate) playback_repetition_policy: AnimationPlaybackRepetitionPolicy,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub(crate) struct AnimationGraphNodePlaybackRequestRejected {
    pub(crate) animation_subject_entity: Entity,
}

/// An explicit clip request that could not start on any matching presentation.
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub(crate) struct AnimationClipPlaybackRequestRejected {
    pub(crate) request_id: usize,
    pub(crate) animation_subject_entity: Entity,
    pub(crate) animation_clip_asset_key: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AnimationGraphNodePlaybackRequestRejectionReason {
    MissingAnimationAsset,
    UnknownAnimationGraphNode,
    MissingAuthoredGraphEdge,
    ActiveTransitionNotInterruptible,
    AnimationGraphDisabled,
}
