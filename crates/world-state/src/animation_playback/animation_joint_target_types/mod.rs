use bevy::prelude::*;

#[derive(Component, Clone, Debug)]
pub struct AnimationJointTarget {
    pub animation_playback_controller_entity: Entity,
    pub joint_asset_key: String,
}

/// A tool an animation put in an actor's hand, such as a worker's broom. It
/// keeps its own model and doesn't take its holder's animations.
#[derive(Component, Clone, Copy, Debug)]
pub struct HeldPropPresentation;
