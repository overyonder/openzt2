use bevy::prelude::*;

/// Deliver text-key object effects before the task consumes clip completion.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AnimationObjectCommandDelivery;

#[derive(Message, Clone, Debug)]
pub(crate) struct AnimationObjectCommand {
    pub(crate) controller: Entity,
    pub(crate) playback_generation: u64,
    pub(crate) request_id: Option<usize>,
    pub(crate) joint: Entity,
    pub(crate) action:
        openzt2_game_data::animation::animation_text_key::AuthoredAnimationTextAction,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub(crate) struct AnimationCompleted {
    pub(crate) animation_playback_controller_entity: Entity,
    pub(crate) playback_generation: u64,
    pub(crate) explicit_clip_request_id: Option<usize>,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub(crate) struct AnimationTraversalTransitionRequested {
    pub(crate) animation_playback_controller_entity: Entity,
    pub(crate) animation_graph_node_asset_key: String,
}
