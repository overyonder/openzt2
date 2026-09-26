use bevy::prelude::*;

/// Links a renderable animation playback controller to the gameplay entity
/// whose state it presents.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AnimationPresentationOwner {
    pub(crate) gameplay_entity: Entity,
}
