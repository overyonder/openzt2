use bevy::prelude::*;

use crate::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;

/// Typed gameplay or presentation intent to enter a compiled graph node.
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct AnimationGraphNodePlaybackRequest {
    pub animation_subject_entity: Entity,
    pub animation_graph_node_asset_key: String,
    pub blend_duration_milliseconds: Option<u32>,
    pub preserves_current_playback_time: bool,
    pub enters_immediately: bool,
}

impl AnimationGraphNodePlaybackRequest {
    pub fn new(animation_subject_entity: Entity, animation_graph_node_asset_key: String) -> Self {
        Self {
            animation_subject_entity,
            animation_graph_node_asset_key,
            blend_duration_milliseconds: None,
            preserves_current_playback_time: false,
            enters_immediately: false,
        }
    }

    pub fn with_authored_transition_and_time_policy(
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
pub struct AnimationClipPlaybackRequest {
    pub animation_subject_entity: Entity,
    pub animation_clip_asset_key: String,
    pub blend_duration_milliseconds: u32,
    pub playback_speed_permille: i16,
    pub playback_repetition_policy: AnimationPlaybackRepetitionPolicy,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct AnimationGraphNodePlaybackRequestRejected {
    pub animation_subject_entity: Entity,
}

/// An explicit clip request that could not start on any matching presentation.
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct AnimationClipPlaybackRequestRejected {
    pub request_id: usize,
    pub animation_subject_entity: Entity,
    pub animation_clip_asset_key: String,
}

/// Withdraw one explicit request, including any demand-loading wait.
#[derive(Message, Clone, Copy, Debug)]
pub struct CancelAnimationClipPlaybackRequest {
    pub animation_subject_entity: Entity,
    pub request_id: usize,
}

/// The presentation that actually accepted an explicit request and its prior node.
#[derive(Message, Clone, Debug)]
pub struct AnimationClipPlaybackRequestStarted {
    pub animation_subject_entity: Entity,
    pub request_id: usize,
    pub playback_generation: u64,
    pub previous_graph_node: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationGraphNodePlaybackRequestRejectionReason {
    MissingAnimationAsset,
    UnknownAnimationGraphNode,
    MissingAuthoredGraphEdge,
    ActiveTransitionNotInterruptible,
    AnimationGraphDisabled,
}
