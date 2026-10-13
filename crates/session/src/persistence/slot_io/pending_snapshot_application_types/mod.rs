use bevy::prelude::{Component, Entity};
use openzt2_game_data::AssetId;

use openzt2_world_state::game_session_types::WorldSessionMode;

use openzt2_interface_state::persistence::save_slot_types::SaveSlotId;

#[derive(Component, Debug, Clone)]
pub(crate) struct PendingSnapshotApplication {
    pub(in crate::persistence) save_slot_identifier: SaveSlotId,
    pub(in crate::persistence) scenario_definition_identifier: Option<AssetId>,
    pub(in crate::persistence) session_mode: Option<WorldSessionMode>,
    pub(in crate::persistence) baseline_world_load_requested: bool,
    pub(in crate::persistence) baseline_world_load_completed: bool,
    pub(in crate::persistence) baseline_world_root: Option<Entity>,
    pub(super) maintenance_prefabs:
        Vec<bevy::prelude::Handle<openzt2_game_assets::scene_prefab::ScenePrefabAsset>>,
}
