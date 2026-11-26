use bevy::prelude::*;

#[derive(Component, Clone, Copy, PartialEq)]
pub(crate) struct StandardBevyAnimationJointBlendPose {
    pub(super) playback_generation: u64,
    pub(super) pose: Transform,
}
