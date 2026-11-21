use bevy::prelude::*;

#[derive(Component, Clone, Copy)]
pub(super) struct StandardBevyAnimationJointBlendPose {
    pub(super) playback_generation: u64,
    pub(super) pose: Transform,
}
