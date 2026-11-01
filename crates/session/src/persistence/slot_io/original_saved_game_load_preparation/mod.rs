use super::{
    pending_snapshot_application_types::PendingSnapshotApplication,
    persistence_slot_filesystem_task_types::PersistenceSlotFilesystemTask,
    terrain_snapshot_types::TerrainSnapshotRecord, world_snapshot_types::WorldSnapshotRecord,
};
use bevy::prelude::*;
use openzt2_game_assets::{
    original_saved_game_asset_loading::OriginalSavedGameAsset,
    original_saved_game_source::OriginalSavedGameSources,
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_asset_preparation::WorldAssetPreparation,
    world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
};
use openzt2_game_data::AssetId;
use openzt2_interface_state::persistence::{
    profile_types::ProfileOptions,
    save_slot_types::SaveSlotCatalogue,
    saved_game_source_types::{LoadOriginalSavedGame, OriginalSavedGameLoadFailed},
};

#[derive(Component)]
pub(in crate::persistence) struct OriginalSavedGameLoadPreparation {
    source: AssetId,
    profile: AssetId,
    asset: Handle<OriginalSavedGameAsset>,
    timestamp: u64,
    world: Option<(WorldSnapshotRecord, TerrainSnapshotRecord)>,
    document: Option<Handle<WorldScenarioDocumentAsset>>,
}

pub(in crate::persistence) fn begin_original_saved_game_loading(
    mut commands: Commands,
    mut requests: MessageReader<LoadOriginalSavedGame>,
    profile: Res<ProfileOptions>,
    catalogue: Res<SaveSlotCatalogue>,
    server: Res<AssetServer>,
    active: Query<
        (),
        Or<(
            With<PersistenceSlotFilesystemTask>,
            With<PendingSnapshotApplication>,
            With<OriginalSavedGameLoadPreparation>,
        )>,
    >,
    mut failures: MessageWriter<OriginalSavedGameLoadFailed>,
) {
    let mut accepted = false;
    for request in requests.read() {
        let record = catalogue
            .original_saved_game_records
            .iter()
            .find(|record| record.source_identifier == request.source_identifier);
        let failure = if accepted || !active.is_empty() {
            Some("another persistence operation is active")
        } else if record.is_none() {
            Some("saved game is absent from the current catalogue")
        } else {
            None
        };
        if let Some(reason) = failure {
            failures.write(OriginalSavedGameLoadFailed {
                source_identifier: request.source_identifier,
                reason: reason.into(),
            });
            continue;
        }
        let Some(record) = record else {
            continue;
        };
        let path = OriginalSavedGameSources::member_path(
            request.source_identifier,
            std::path::Path::new("saved/world.zt2"),
        );
        commands.spawn((
            OriginalSavedGameLoadPreparation {
                source: request.source_identifier,
                profile: profile.profile_identifier,
                asset: server.load(format!("original-saves://{}", path.display())),
                timestamp: record.last_saved_unix_timestamp_milliseconds,
                world: None,
                document: None,
            },
            WorldAssetPreparation,
        ));
        accepted = true;
    }
}

pub(super) fn prepare_original_saved_world_assets(
    preparation: &mut OriginalSavedGameLoadPreparation,
    assets: &Assets<OriginalSavedGameAsset>,
    terrains: &Assets<TerrainAsset>,
    server: &AssetServer,
    saved_world: &mut super::saved_world_snapshot_preparation::SavedWorldSnapshotPreparation,
) -> Result<bool, String> {
    if let Some(bevy::asset::LoadState::Failed(error)) =
        server.get_load_state(preparation.asset.id())
    {
        return Err(format!("cannot load original saved world: {error}"));
    }
    let Some(saved) = assets.get(&preparation.asset) else {
        return Ok(false);
    };
    if let Some(bevy::asset::LoadState::Failed(error)) = server.get_load_state(saved.terrain.id()) {
        return Err(format!("cannot load saved terrain: {error}"));
    }
    let Some(terrain) = terrains.get(&saved.terrain) else {
        return Ok(false);
    };
    if preparation.world.is_none() {
        preparation.world = Some(
            super::original_saved_game_world_conversion::read_original_saved_world(
                saved,
                terrain,
                preparation.timestamp,
            )
            .map_err(|error| error.to_string())?,
        );
    }
    let Some((world, terrain)) = &preparation.world else {
        return Ok(false);
    };
    saved_world
        .prepare(world, Some(terrain), &mut preparation.document)
        .map_err(|error| format!("cannot prepare saved-world assets: {error:?}"))
}

#[derive(bevy::ecs::system::SystemParam)]
pub(in crate::persistence) struct OriginalSavedGamePreparationAssets<'w> {
    originals: Res<'w, Assets<OriginalSavedGameAsset>>,
    terrains: Res<'w, Assets<TerrainAsset>>,
    server: Res<'w, AssetServer>,
    definitions: Res<'w, Assets<openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset>>,
    definitions_index: Res<'w, openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions>,
    species: Res<'w, Assets<openzt2_game_assets::species::species_asset_types::SpeciesAsset>>,
    species_index: Res<'w, openzt2_game_assets::species::species_asset_types::SpeciesAssets>,
    audio: Res<'w, Assets<openzt2_game_assets::audio::audio_asset_types::AudioAsset>>,
    audio_index: Res<'w, openzt2_game_assets::audio::audio_asset_types::AudioAssets>,
    scripts: Res<'w, Assets<openzt2_game_assets::lua_script::lua_script_asset_loading::LuaScriptAsset>>,
    script_index: Res<'w, openzt2_game_assets::lua_script::lua_script_archive_path_index::LuaScriptArchivePathIndex>,
}

pub(in crate::persistence) fn complete_original_saved_game_preparation(
    mut commands: Commands,
    mut preparations: Query<(Entity, &mut OriginalSavedGameLoadPreparation)>,
    profile: Res<ProfileOptions>,
    assets: OriginalSavedGamePreparationAssets,
    mut saved_world: super::saved_world_snapshot_preparation::SavedWorldSnapshotPreparation,
    mut scratch: ResMut<super::super::snapshot_container_types::ReusableWorldSnapshotByteBuffer>,
    mut failures: MessageWriter<OriginalSavedGameLoadFailed>,
) {
    for (operation, mut preparation) in &mut preparations {
        let result = (|| -> Result<Option<Vec<u8>>, String> {
            if profile.profile_identifier != preparation.profile {
                return Err("the active profile changed while preparing the saved game".into());
            }
            if !prepare_original_saved_world_assets(
                &mut preparation,
                &assets.originals,
                &assets.terrains,
                &assets.server,
                &mut saved_world,
            )? {
                return Ok(None);
            }
            if assets
                .script_index
                .has_pending_script_loads(&assets.server, &assets.scripts)
            {
                return Ok(None);
            }
            let Some(definitions) = assets.definitions_index.get(&assets.definitions) else {
                return Ok(None);
            };
            let Some(species) = assets.species_index.get(&assets.species) else {
                return Ok(None);
            };
            let Some(audio) = assets.audio_index.get(&assets.audio) else {
                return Ok(None);
            };
            let crowd = audio
                .crowd()
                .copied()
                .ok_or_else(|| "world crowd audio policy is unavailable".to_owned())?;
            let saved = assets
                .originals
                .get(&preparation.asset)
                .ok_or_else(|| "prepared original saved-game asset was removed".to_owned())?;
            let (world, terrain) = preparation
                .world
                .as_ref()
                .ok_or_else(|| "prepared original world is absent".to_owned())?;
            let document = preparation
                .document
                .as_ref()
                .ok_or_else(|| "prepared saved-world document is absent".to_owned())?;
            super::original_saved_game_snapshot_conversion::encode_original_saved_game_snapshot(
                saved,
                world,
                terrain,
                preparation.profile,
                document,
                &assets.scripts,
                &assets.script_index,
                definitions,
                species,
                crowd,
            )
            .map(Some)
            .map_err(|error| error.to_string())
        })();
        match result {
            Ok(None) => {}
            Err(reason) => {
                failures.write(OriginalSavedGameLoadFailed {
                    source_identifier: preparation.source,
                    reason,
                });
                commands.entity(operation).despawn();
            }
            Ok(Some(bytes)) => {
                scratch.snapshot_container_bytes = bytes;
                scratch.contains_validated_snapshot_container = false;
                commands.entity(operation).remove::<OriginalSavedGameLoadPreparation>().insert(PendingSnapshotApplication {
                    world_snapshot:None,
                    terrain_snapshot:None,
                    saved_world_document:preparation.document.clone(),
                    source:openzt2_interface_state::persistence::saved_game_source_types::SavedGameSource::Original(preparation.source),
                    terrain_restored:false,behavior_documents:Vec::new(),scenario_definition_identifier:None,session_mode:None,
                    baseline_world_load_requested:false,baseline_world_load_completed:false,baseline_world_root:None,
                    maintenance_prefabs:Vec::new(),placement_hydration:None,
                });
            }
        }
    }
}
