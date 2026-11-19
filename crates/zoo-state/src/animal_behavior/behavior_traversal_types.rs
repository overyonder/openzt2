use bevy::{curve::cubic_splines::CubicSegment, prelude::*};

/// A named path node, local to the object that owns the traversal geometry.
#[derive(Component)]
pub struct BehaviorTraversalPath {
    pub curve: CubicSegment<Vec3>,
    pub start_rotation: Quat,
    pub end_rotation: Quat,
    pub enter_clip: String,
    pub exit_clip: String,
}
