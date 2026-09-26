use bevy::prelude::Component;
use openzt2_game_data::AssetId;

use crate::game_session_types::WorldSessionMode;

use super::super::save_slot_types::SaveSlotId;

#[derive(Component, Debug, Clone, Copy)]
pub(in crate::plugins::persistence) struct PendingSnapshotApplication {
    pub(in crate::plugins::persistence) save_slot_identifier: SaveSlotId,
    pub(in crate::plugins::persistence) scenario_definition_identifier: Option<AssetId>,
    pub(in crate::plugins::persistence) session_mode: Option<WorldSessionMode>,
    pub(in crate::plugins::persistence) baseline_world_load_requested: bool,
    pub(in crate::plugins::persistence) baseline_world_load_completed: bool,
}
