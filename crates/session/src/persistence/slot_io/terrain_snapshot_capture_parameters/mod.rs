use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
    terrain_snapshot_capture::capture_terrain_snapshot_record,
    terrain_snapshot_types::TerrainSnapshotRecord,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_world_state::world_spawn::{
    selected_world_terrain_asset_handle::SelectedWorldTerrainAssetHandle,
    world_membership_types::{WorldMember, WorldRoot},
};
use openzt2_zoo_state::terrain::{
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk},
    terrain_water_presentation_types::{
        TerrainWaterRegionQuery, TerrainWaterRegionRetired, TerrainWaterRegionSource,
    },
};

#[derive(SystemParam)]
pub(super) struct TerrainSnapshotCapture<'w, 's> {
    assets: Option<Res<'w, Assets<TerrainAsset>>>,
    roots: Query<'w, 's, (Entity, &'static SelectedWorldTerrainAssetHandle), With<WorldRoot>>,
    chunks: Query<
        'w,
        's,
        (
            &'static WorldMember,
            &'static TerrainChunk,
            Option<&'static EditedTerrainSamples>,
        ),
    >,
    members: Query<'w, 's, &'static WorldMember>,
    parents: Query<'w, 's, &'static ChildOf>,
    regions: TerrainWaterRegionQuery<'w, 's>,
    sources: Query<
        'w,
        's,
        (
            Entity,
            &'static TerrainWaterRegionSource,
            Has<TerrainWaterRegionRetired>,
        ),
    >,
}

impl TerrainSnapshotCapture<'_, '_> {
    pub(super) fn capture(&self) -> Result<TerrainSnapshotRecord, Failure> {
        let (root, terrain) = self
            .roots
            .single()
            .map_err(|_| Failure::SnapshotTransactionInFlight)?;
        let asset = self
            .assets
            .as_deref()
            .and_then(|assets| assets.get(&terrain.0))
            .ok_or(Failure::SnapshotTransactionInFlight)?;
        let belongs = |owner| self.owner_is_in_world(owner, root);
        capture_terrain_snapshot_record(
            &terrain.0,
            asset,
            self.chunks
                .iter()
                .filter(|(member, _, _)| member.root == root)
                .map(|(_, chunk, edited)| (chunk, edited)),
            &self.regions,
            belongs,
            self.sources
                .iter()
                .filter(|(owner, _, retired)| !retired && belongs(*owner))
                .map(|(owner, source, _)| (owner, source)),
            self.sources
                .iter()
                .filter(|(owner, _, retired)| *retired && belongs(*owner))
                .map(|(_, source, _)| source),
        )
    }

    fn owner_is_in_world(&self, entity: Entity, root: Entity) -> bool {
        entity == root
            || self
                .members
                .get(entity)
                .is_ok_and(|member| member.root == root)
            || self
                .parents
                .iter_ancestors::<ChildOf>(entity)
                .any(|ancestor| {
                    ancestor == root
                        || self
                            .members
                            .get(ancestor)
                            .is_ok_and(|member| member.root == root)
                })
    }
}
