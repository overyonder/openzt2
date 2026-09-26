use bevy::prelude::{Component, Entity, Handle};
use openzt2_game_data::AssetId;

use crate::assets::scene_prefab::ScenePrefabAsset;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

use super::world_load_failure::WorldLoadFailure;

#[derive(Component, Debug)]
pub(crate) struct WorldHydration {
    scenario: AssetId,
    start_asset: Handle<WorldScenarioDocumentAsset>,
    start: AssetId,
    record_count: usize,
    next_record: usize,
    ids_reserved: bool,
    terrain_ready: bool,
    paths_ready: bool,
    topology_ready: bool,
    values_ready: bool,
    /// The one prefab currently being hydrated. Retaining its strong handle
    /// lets Bevy finish the asynchronous load instead of restarting it every
    /// frame while the catalogue itself remains metadata-only.
    pending_prefab: Option<(AssetId, Handle<ScenePrefabAsset>)>,
    pending_named_prefabs: Vec<(AssetId, Handle<ScenePrefabAsset>)>,
    spawned_roots: Vec<Entity>,
    failure: Option<WorldLoadFailure>,
}

impl WorldHydration {
    pub(crate) const fn prefab_loading_progress(&self) -> (usize, usize) {
        (self.next_record, self.record_count)
    }

    pub(super) fn new_pending_world_hydration(
        scenario: AssetId,
        start_asset: Handle<WorldScenarioDocumentAsset>,
        start: AssetId,
        record_count: usize,
    ) -> Self {
        Self {
            scenario,
            start_asset,
            start,
            record_count,
            next_record: 0,
            ids_reserved: false,
            terrain_ready: false,
            paths_ready: false,
            topology_ready: false,
            values_ready: false,
            pending_prefab: None,
            pending_named_prefabs: Vec::new(),
            spawned_roots: Vec::with_capacity(record_count),
            failure: None,
        }
    }

    pub(super) const fn starting_zoo_document_asset_handle(
        &self,
    ) -> &Handle<WorldScenarioDocumentAsset> {
        &self.start_asset
    }

    pub(super) const fn starting_zoo_id(&self) -> AssetId {
        self.start
    }

    pub(super) const fn selected_scenario_id(&self) -> AssetId {
        self.scenario
    }

    pub(super) const fn failure(&self) -> Option<WorldLoadFailure> {
        self.failure
    }

    pub(super) fn record_failure(&mut self, failure: WorldLoadFailure) {
        self.failure = Some(failure);
    }

    pub(super) fn persistent_id_reservation_is_pending(&self) -> bool {
        !self.ids_reserved && self.failure.is_none()
    }

    pub(super) fn record_persistent_id_reservation_completion(&mut self) {
        self.ids_reserved = true;
    }

    pub(super) fn spawned_world_prefab_root_at_record_index(&self, index: u32) -> Option<Entity> {
        self.spawned_roots.get(index as usize).copied()
    }

    pub(super) fn record_world_prefab_spawn_completion(&mut self, root: Entity) {
        self.spawned_roots.push(root);
        self.next_record += 1;
        self.pending_prefab = None;
        self.pending_named_prefabs.clear();
    }

    pub(super) fn world_prefab_record_hydration_is_pending(&self) -> bool {
        self.ids_reserved && self.failure.is_none()
    }

    pub(super) const fn next_world_prefab_record_index(&self) -> usize {
        self.next_record
    }

    pub(super) fn bounded_world_prefab_record_batch_end(&self, batch_size: usize) -> usize {
        self.next_record
            .saturating_add(batch_size)
            .min(self.record_count)
    }

    pub(super) fn retained_pending_prefab(&self) -> Option<&(AssetId, Handle<ScenePrefabAsset>)> {
        self.pending_prefab.as_ref()
    }

    pub(super) fn retain_pending_prefab(&mut self, id: AssetId, handle: Handle<ScenePrefabAsset>) {
        self.pending_prefab = Some((id, handle));
    }

    pub(super) fn retained_pending_named_prefab(
        &self,
        id: AssetId,
    ) -> Option<&Handle<ScenePrefabAsset>> {
        self.pending_named_prefabs
            .iter()
            .find(|(candidate, _)| *candidate == id)
            .map(|(_, handle)| handle)
    }

    pub(super) fn retain_pending_named_prefab(
        &mut self,
        id: AssetId,
        handle: Handle<ScenePrefabAsset>,
    ) {
        self.pending_named_prefabs.push((id, handle));
    }

    pub(crate) fn terrain_hydration_is_pending(&self) -> bool {
        !self.terrain_ready && self.failure.is_none()
    }

    pub(super) fn record_terrain_hydration_completion(&mut self) {
        self.terrain_ready = true;
    }

    pub(super) fn starting_path_hydration_is_pending(&self) -> bool {
        !self.paths_ready && self.terrain_ready && self.ids_reserved && self.failure.is_none()
    }

    pub(super) fn record_starting_path_hydration_completion(&mut self) {
        self.paths_ready = true;
    }

    pub(super) fn starting_topology_hydration_is_pending(&self) -> bool {
        !self.topology_ready && self.terrain_ready && self.ids_reserved && self.failure.is_none()
    }

    pub(super) fn record_starting_topology_hydration_completion(&mut self) {
        self.topology_ready = true;
    }

    pub(super) fn authored_world_spawn_value_application_is_pending(&self) -> bool {
        !self.values_ready && self.failure.is_none() && self.next_record == self.record_count
    }

    pub(super) fn record_authored_world_spawn_value_application_completion(&mut self) {
        self.values_ready = true;
    }

    pub(super) fn is_complete(&self) -> bool {
        self.terrain_ready
            && self.ids_reserved
            && self.topology_ready
            && self.paths_ready
            && self.values_ready
            && self.next_record == self.record_count
    }
}
