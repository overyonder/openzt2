use std::time::Duration;

use bevy::{animation::graph::AnimationNodeIndex, prelude::*};

#[derive(Component)]
pub struct PrefabRealTimeRotationClock {
    pub node: AnimationNodeIndex,
    pub origin: Option<Duration>,
}
