use bevy::prelude::*;

#[derive(Component, Clone, Debug)]
pub(crate) struct AnimationJointTarget {
    pub(crate) animation_playback_controller_entity: Entity,
    pub(crate) joint_asset_key: String,
}
