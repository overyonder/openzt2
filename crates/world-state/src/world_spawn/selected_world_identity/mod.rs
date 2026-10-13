use bevy::prelude::Component;
use openzt2_game_data::AssetId;

use crate::game_session_types::WorldSessionMode;

/// Immutable identity of the selected play request and its resolved asset
/// start. This remains on the world root so freeform maps do not collapse into
/// a scenario-shaped identity during load or persistence.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedWorldIdentity {
    pub requested: AssetId,
    pub map: AssetId,
    pub start: AssetId,
    pub mode: WorldSessionMode,
    pub profile: AssetId,
}
