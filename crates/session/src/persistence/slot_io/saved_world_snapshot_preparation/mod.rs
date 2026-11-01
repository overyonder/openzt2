use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
    terrain_snapshot_types::TerrainSnapshotRecord, world_snapshot_types::WorldSnapshotRecord,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::{
    saved_world_asset_source::SavedWorldAssetSource,
    world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
};

#[derive(SystemParam)]
pub(in crate::persistence) struct SavedWorldSnapshotPreparation<'w> {
    server: Option<Res<'w, AssetServer>>,
    source: Option<ResMut<'w, SavedWorldAssetSource>>,
}

impl SavedWorldSnapshotPreparation<'_> {
    pub(super) fn prepare(
        &mut self,
        world: &WorldSnapshotRecord,
        terrain: Option<&TerrainSnapshotRecord>,
        retained: &mut Option<Handle<WorldScenarioDocumentAsset>>,
    ) -> Result<bool, Failure> {
        let Some(document) = &world.source_document else {
            return Ok(true);
        };
        let terrain = terrain.ok_or(Failure::CorruptSnapshotSection)?;
        let expected = if let Some(map) = document
            .document
            .find_world_map(world.scenario_definition_identifier)
        {
            Some((map.id, map.starting_zoo))
        } else {
            document
                .document
                .campaigns
                .iter()
                .flat_map(|campaign| &campaign.scenarios)
                .find(|scenario| scenario.id == world.scenario_definition_identifier)
                .map(|scenario| (scenario.map, scenario.starting_zoo))
        };
        if world.requested_world_definition_identifier != world.scenario_definition_identifier
            || expected
                != Some((
                    world.map_definition_identifier,
                    world.starting_point_definition_identifier,
                ))
            || document
                .document
                .find_starting_zoo(world.starting_point_definition_identifier)
                .is_none()
        {
            return Err(Failure::BrokenPersistentEntityReference);
        }
        let server = self
            .server
            .as_deref()
            .ok_or(Failure::UnknownAssetIdentifier)?;
        if retained.is_none() {
            let map = document
                .document
                .maps
                .iter()
                .find(|map| map.id == world.map_definition_identifier)
                .ok_or(Failure::UnknownAssetIdentifier)?;
            let source = self
                .source
                .as_deref_mut()
                .ok_or(Failure::UnknownAssetIdentifier)?;
            *retained = Some(
                source
                    .register(document.clone(), map.terrain, &terrain.grid, server)
                    .map_err(|_| Failure::CorruptSnapshotSection)?,
            );
        }
        let handle = retained.as_ref().ok_or(Failure::UnknownAssetIdentifier)?;
        if matches!(
            server.get_load_state(handle.id()),
            Some(bevy::asset::LoadState::Failed(_))
        ) || matches!(
            server.get_recursive_dependency_load_state(handle.id()),
            Some(bevy::asset::RecursiveDependencyLoadState::Failed(_))
        ) {
            return Err(Failure::UnknownAssetIdentifier);
        }
        Ok(server.is_loaded_with_dependencies(handle.id()))
    }
}
