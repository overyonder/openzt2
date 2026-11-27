use super::pending_snapshot_application_types::SnapshotTerrainAsset;
use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
    terrain_snapshot_types::TerrainSnapshotRecord, world_snapshot_types::WorldSnapshotRecord,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::{
    asset_source::AssetArchives,
    terrain::canonical_terrain_asset_loading::add_restored_terrain_asset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
    world_scenario::saved_world_scenario_asset_construction::add_restored_world_scenario_asset,
    world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
};

#[derive(SystemParam)]
pub(in crate::persistence) struct SavedWorldSnapshotPreparation<'w> {
    server: Option<Res<'w, AssetServer>>,
    archives: Option<Res<'w, AssetArchives>>,
    definitions: Option<Res<'w, WorldDefinitions>>,
    definition_assets: Option<Res<'w, Assets<WorldDefinitionAsset>>>,
}

impl SavedWorldSnapshotPreparation<'_> {
    pub(super) fn mode_document(
        &self,
        world: &WorldSnapshotRecord,
    ) -> Result<Option<super::pending_snapshot_application_types::SnapshotModeDocument>, Failure>
    {
        let Some(path) = &world.mode_document_path else {
            return Ok(None);
        };
        let server = self
            .server
            .as_deref()
            .ok_or(Failure::UnknownAssetIdentifier)?;
        let handle = server.load(path.clone());
        Ok(Some(
            super::pending_snapshot_application_types::SnapshotModeDocument(handle),
        ))
    }

    pub(super) fn prepare(
        &self,
        world: &WorldSnapshotRecord,
        terrain: Option<&TerrainSnapshotRecord>,
        retained: &mut Option<Handle<WorldScenarioDocumentAsset>>,
    ) -> Result<Option<SnapshotTerrainAsset>, Failure> {
        let Some(document) = &world.source_document else {
            return Ok(None);
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
        if retained.is_some() {
            return Ok(None);
        }
        let server = self
            .server
            .as_deref()
            .ok_or(Failure::UnknownAssetIdentifier)?;
        let map = document
            .document
            .find_world_map(world.map_definition_identifier)
            .ok_or(Failure::UnknownAssetIdentifier)?;
        let archives = self
            .archives
            .as_deref()
            .ok_or(Failure::UnknownAssetIdentifier)?;
        let declaring_path = document
            .terrains
            .iter()
            .find(|terrain| terrain.id == map.terrain)
            .map(|terrain| std::path::PathBuf::from(terrain.path.as_ref()))
            .ok_or(Failure::UnknownAssetIdentifier)?;
        let tank_water_source_paths = self
            .definitions
            .as_deref()
            .zip(self.definition_assets.as_deref())
            .and_then(|(definitions, assets)| definitions.get(assets))
            .into_iter()
            .flat_map(|definitions| definitions.tank_water_material_paths())
            .map(std::path::PathBuf::from)
            .collect();
        let terrain = add_restored_terrain_asset(
            server,
            archives,
            terrain.grid.clone(),
            declaring_path,
            tank_water_source_paths,
        );
        let scenario = add_restored_world_scenario_asset(
            server,
            archives,
            document.clone(),
            map.terrain,
            terrain.clone(),
        )
        .map_err(|_| Failure::CorruptSnapshotSection)?;
        *retained = Some(scenario);
        Ok(Some(SnapshotTerrainAsset(terrain)))
    }
}
