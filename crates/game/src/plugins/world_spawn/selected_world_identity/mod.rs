use bevy::prelude::Component;
use openzt2_game_data::AssetId;

use crate::game_session_types::WorldSessionMode;

/// Immutable identity of the selected play request and its resolved asset
/// start. This remains on the world root so freeform maps do not collapse into
/// a scenario-shaped identity during load or persistence.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SelectedWorldIdentity {
    pub(crate) requested: AssetId,
    pub(crate) map: AssetId,
    pub(crate) start: AssetId,
    pub(crate) mode: WorldSessionMode,
    pub(crate) profile: AssetId,
}
