use bevy::prelude::*;

#[derive(Component, Clone, Debug)]
pub struct AnimationJointTarget {
    pub animation_playback_controller_entity: Entity,
    pub joint_asset_key: String,
}
