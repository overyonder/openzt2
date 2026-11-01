use bevy::prelude::{Component, Entity};
use openzt2_game_data::AssetId;

use openzt2_world_state::game_session_types::WorldSessionMode;

use openzt2_interface_state::persistence::save_slot_types::SaveSlotId;

#[derive(Component, Clone)]
pub(crate) struct PendingSnapshotApplication {
    pub(super) world_snapshot: Option<super::world_snapshot_types::WorldSnapshotRecord>,
    pub(super) terrain_snapshot: Option<super::terrain_snapshot_types::TerrainSnapshotRecord>,
    pub(super) saved_world_document: Option<bevy::prelude::Handle<openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset>>,
    pub(super) terrain_restored: bool,
    pub(super) behavior_documents: Vec<
        bevy::prelude::Handle<
            openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset,
        >,
    >,
    pub(super) placement_hydration: Option<Vec<u64>>,
    pub(in crate::persistence) save_slot_identifier: SaveSlotId,
    pub(in crate::persistence) scenario_definition_identifier: Option<AssetId>,
    pub(in crate::persistence) session_mode: Option<WorldSessionMode>,
    pub(in crate::persistence) baseline_world_load_requested: bool,
    pub(in crate::persistence) baseline_world_load_completed: bool,
    pub(in crate::persistence) baseline_world_root: Option<Entity>,
    pub(super) maintenance_prefabs:
        Vec<bevy::prelude::Handle<openzt2_game_assets::scene_prefab::ScenePrefabAsset>>,
}

impl std::fmt::Debug for PendingSnapshotApplication {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PendingSnapshotApplication")
            .field("slot", &self.save_slot_identifier)
            .field("world_decoded", &self.world_snapshot.is_some())
            .field("terrain_decoded", &self.terrain_snapshot.is_some())
            .field("saved_world_document", &self.saved_world_document)
            .field("terrain_restored", &self.terrain_restored)
            .field("scenario", &self.scenario_definition_identifier)
            .field("mode", &self.session_mode)
            .field("baseline_requested", &self.baseline_world_load_requested)
            .field("baseline_completed", &self.baseline_world_load_completed)
            .field("baseline_root", &self.baseline_world_root)
            .finish()
    }
}
