use bevy::{animation::graph::AnimationNodeIndex, prelude::*};

#[derive(Component)]
pub enum PrefabTransformPlaybackClock {
    Instance(AnimationNodeIndex),
    Shared(AnimationNodeIndex),
}
