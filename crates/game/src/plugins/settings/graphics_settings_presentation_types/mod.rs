use bevy::prelude::*;

/// An authored text shadow held back while the font shadow option is off.
#[derive(Component, Debug, Clone, Copy)]
pub(super) struct SuppressedAuthoredTextShadow(pub(super) TextShadow);
