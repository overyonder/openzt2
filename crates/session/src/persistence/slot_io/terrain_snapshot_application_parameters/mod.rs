use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
    terrain_snapshot_application::apply_terrain_snapshot_record,
    terrain_snapshot_types::TerrainSnapshotRecord,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_world_state::world_spawn::{
    selected_world_terrain_asset_handle::SelectedWorldTerrainAssetHandle,
    world_membership_types::WorldMember,
};
use openzt2_zoo_state::terrain::{
    terrain_chunk_types::TerrainChunk,
    terrain_edit_types::{TerrainChanged, TerrainPresentationRestored, TerrainWaterChanged},
    terrain_navigation_change_types::TerrainNavigationRestored,
    terrain_water_presentation_types::TerrainWaterRegionSource,
};

#[derive(SystemParam)]
pub(in crate::persistence) struct TerrainSnapshotApplicationParameters<'w, 's> {
    graph: Option<
        Res<
            'w,
            openzt2_zoo_state::locomotion::locomotion_types::ActiveTerrainDerivedNavigationGraph,
        >,
    >,
    roots: Query<'w, 's, &'static SelectedWorldTerrainAssetHandle>,
    chunks: Query<'w, 's, (Entity, &'static WorldMember, &'static TerrainChunk)>,
    members: Query<'w, 's, &'static WorldMember>,
    parents: Query<'w, 's, &'static ChildOf>,
    sources: Query<'w, 's, (Entity, &'static TerrainWaterRegionSource)>,
    changed: Option<ResMut<'w, Messages<TerrainChanged>>>,
    water: Option<ResMut<'w, Messages<TerrainWaterChanged>>>,
    navigation: Option<ResMut<'w, Messages<TerrainNavigationRestored>>>,
    presentation: Option<ResMut<'w, Messages<TerrainPresentationRestored>>>,
}

impl TerrainSnapshotApplicationParameters<'_, '_> {
    pub(super) fn apply(
        &mut self,
        record: &TerrainSnapshotRecord,
        root: Entity,
        transaction: Entity,
        commands: &mut Commands,
    ) -> Result<(), Failure> {
        if self.graph.is_none() {
            return Err(Failure::SnapshotTransactionInFlight);
        }
        let terrain = self
            .roots
            .get(root)
            .map_err(|_| Failure::BrokenPersistentEntityReference)?;
        let belongs = |owner| {
            owner == root
                || self
                    .members
                    .get(owner)
                    .is_ok_and(|member| member.root == root)
                || self
                    .parents
                    .iter_ancestors::<ChildOf>(owner)
                    .any(|ancestor| {
                        ancestor == root
                            || self
                                .members
                                .get(ancestor)
                                .is_ok_and(|member| member.root == root)
                    })
        };
        let changed = self
            .changed
            .as_deref_mut()
            .ok_or(Failure::SnapshotTransactionInFlight)?;
        let water = self
            .water
            .as_deref_mut()
            .ok_or(Failure::SnapshotTransactionInFlight)?;
        let navigation = self
            .navigation
            .as_deref_mut()
            .ok_or(Failure::SnapshotTransactionInFlight)?;
        let presentation = self
            .presentation
            .as_deref_mut()
            .ok_or(Failure::SnapshotTransactionInFlight)?;
        let result = apply_terrain_snapshot_record(
            record,
            &terrain.0,
            root,
            self.chunks
                .iter()
                .filter(|(_, member, _)| member.root == root)
                .map(|(entity, _, chunk)| (entity, chunk)),
            self.sources.iter().filter(|(owner, _)| belongs(*owner)),
            commands,
        )?;
        for bounds in result.changed_chunks {
            changed.write(TerrainChanged {
                transaction,
                chunk: bounds.chunk,
                min: bounds.min,
                max: bounds.max,
            });
            navigation.write(TerrainNavigationRestored {
                chunk: bounds.chunk,
                min: bounds.min,
                max: bounds.max,
            });
            presentation.write(TerrainPresentationRestored {
                chunk: bounds.chunk,
            });
            water.write(bounds);
        }
        Ok(())
    }
}
