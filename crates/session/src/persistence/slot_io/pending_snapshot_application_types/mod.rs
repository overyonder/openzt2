use bevy::prelude::{Component, Entity, Handle};
use openzt2_game_assets::{
    behavior::behavior_asset_types::BehaviorDocumentAsset, scene_prefab::ScenePrefabAsset,
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
};
use openzt2_interface_state::persistence::saved_game_source_types::SavedGameSource;

use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure;
use super::persistence_slot_filesystem_task_types::PersistenceSlotOperation;

#[derive(Component)]
#[require(PersistenceSlotOperation)]
pub(crate) struct PendingSnapshotApplication {
    pub(in crate::persistence) source: SavedGameSource,
    pub(super) saved_world_document: Option<Handle<WorldScenarioDocumentAsset>>,
}

#[derive(Component)]
pub(super) struct SnapshotTerrainAsset(pub(super) Handle<TerrainAsset>);

#[derive(Component)]
pub(super) struct SnapshotBytes(pub(super) Vec<u8>);

#[derive(Component)]
pub(super) struct PrepareSnapshotBaseline;

#[derive(Component)]
pub(super) struct AwaitingSavedWorldDocument;

#[derive(Component)]
pub(super) struct AwaitingWorldRetirement;

#[derive(Component)]
pub(in crate::persistence) struct AwaitingSnapshotWorldLoad;

#[derive(Component)]
pub(super) struct RestoreSnapshotTerrain(pub(super) Entity);

#[derive(Component)]
pub(super) struct AwaitingSnapshotTerrainNavigation {
    pub(super) root: Entity,
}

#[derive(Component)]
pub(super) struct AwaitingSnapshotDomainBaseline(pub(super) Entity);

#[derive(Component)]
pub(super) struct RestoreSnapshotSections {
    pub(super) root: Entity,
    pub(super) prefabs: Vec<Handle<ScenePrefabAsset>>,
    pub(super) behavior_documents: Vec<Handle<BehaviorDocumentAsset>>,
}

#[derive(Component)]
pub(super) struct RestoreSavedFenceEdges(pub(super) Option<bevy::math::IRect>);

#[derive(Component)]
pub(super) struct RestoreTransportIdentities;

#[derive(Component)]
pub(super) struct AttachSavedWorldState {
    pub(super) root: Entity,
    pub(super) behavior_documents: Vec<Handle<BehaviorDocumentAsset>>,
}

#[derive(Component)]
pub(super) struct RestoreTransportState {
    pub(super) records: super::transport_snapshot_types::TransportSnapshotRecords,
}

#[derive(Component)]
pub(super) struct AttachSnapshotEconomy(
    pub(super) Option<super::economy_snapshot_types::EconomySnapshotRecords>,
);

#[derive(Component)]
pub(super) struct AttachSnapshotMaintenance(
    pub(super) Vec<super::maintenance_snapshot_types::MaintainedObjectSnapshotRecord>,
);

#[derive(Component)]
pub(super) struct AttachSnapshotDonations(
    pub(super) Vec<super::donation_snapshot_section::DonationSnapshotRecord>,
);

#[derive(Component)]
pub(super) struct RestoreRuntimeContinuation {
    pub(super) record:
        Option<super::runtime_continuation_snapshot_types::RuntimeContinuationSnapshot>,
    pub(super) actors: ResumeSnapshotActors,
    pub(super) photo_challenges:
        Vec<super::photo_snapshot_types::PhotoChallengeProgressSnapshotRecord>,
}

#[derive(Component)]
pub(super) struct ResumeSnapshotActors {
    pub(super) root: Entity,
    pub(super) placement_identifiers: Vec<u64>,
    pub(super) behavior_documents: Vec<Handle<BehaviorDocumentAsset>>,
}

#[derive(Component)]
pub(super) struct SnapshotRestored(pub(super) Entity);

#[derive(Component)]
pub(super) enum SnapshotRestoreFailed {
    BeforeReplacement(WorldSnapshotPersistenceFailure),
    DuringReplacement {
        reason: WorldSnapshotPersistenceFailure,
        root_to_unload: Option<Entity>,
    },
}

#[derive(Component)]
pub(super) struct SnapshotSectionRestore<T: Send + Sync + 'static>(pub(super) Option<T>);
