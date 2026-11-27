use bevy::prelude::*;

/// Tools for people making mods, such as the live Blender link. Off by default.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ModderToolsSettings {
    pub enabled: bool,
}
