use bevy::prelude::*;

/// Presentation marker for one source-composed authored biome surface
/// chooser. It retains only the canonical biome identity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiBiomePanel {
    pub biome: openzt2_game_data::AssetId,
}
