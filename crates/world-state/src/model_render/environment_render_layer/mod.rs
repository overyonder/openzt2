use bevy::prelude::*;

/// Ordered sky/sun presentation, separate from ordinary world depth.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnvironmentRenderLayer {
    pub layer: u16,
    pub clear_depth_after: bool,
}
