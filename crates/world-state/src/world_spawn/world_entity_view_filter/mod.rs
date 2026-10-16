use bevy::prelude::*;

/// Suppresses an entity's render tree without changing its gameplay visibility.
#[derive(Component)]
pub struct HiddenByWorldViewFilter;
