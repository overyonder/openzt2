use bevy::prelude::*;

/// Deliver text-key object effects before the task consumes clip completion.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnimationObjectCommandDelivery;

#[derive(Message, Clone, Debug)]
pub struct AnimationObjectCommand {
    pub controller: Entity,
    pub playback_generation: u64,
    pub request_id: Option<usize>,
    pub joint: Entity,
    pub action: openzt2_game_data::animation::animation_text_key::AuthoredAnimationTextAction,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct AnimationCompleted {
    pub animation_playback_controller_entity: Entity,
    pub playback_generation: u64,
    pub explicit_clip_request_id: Option<usize>,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct AnimationTraversalTransitionRequested {
    pub animation_playback_controller_entity: Entity,
    pub animation_graph_node_asset_key: String,
}

#[derive(Message, Clone, Debug)]
pub struct AnimationAiCommandRequested {
    pub controller: Entity,
    pub target: Entity,
    pub command: String,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct AnimationWhapRequested {
    pub controller: Entity,
    pub target: Entity,
    pub angle_degrees: i32,
    pub duration_frames: i32,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct AnimationDockCommandRequested {
    pub controller: Entity,
    pub command: AnimationDockCommand,
}

#[derive(Clone, Copy, Debug)]
pub enum AnimationDockCommand {
    Enable(bool),
    GoToNode(i32),
}
