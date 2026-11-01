use super::{
    terrain_snapshot_types::TerrainSnapshotRecord, world_snapshot_types::WorldSnapshotRecord,
};
use openzt2_game_assets::{
    original_saved_game_asset_loading::OriginalSavedGameAsset,
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
};
use openzt2_game_data::world_scenario::WorldMapSupportedGameModeFlags;
use openzt2_world_state::game_session_types::WorldSessionMode;
use std::io;

pub(super) fn read_original_saved_world(
    saved: &OriginalSavedGameAsset,
    terrain: &TerrainAsset,
    last_saved_unix_timestamp_milliseconds: u64,
) -> io::Result<(WorldSnapshotRecord, TerrainSnapshotRecord)> {
    let [map] = saved.source.document.maps.as_slice() else {
        return Err(invalid("saved world does not contain exactly one map"));
    };
    let [start] = saved.source.document.starting_zoos.as_slice() else {
        return Err(invalid(
            "saved world does not contain exactly one starting world",
        ));
    };
    if start.map != map.id || start.id != map.starting_zoo {
        return Err(invalid("saved map and starting world identities disagree"));
    }
    let mode = if map.supported_game_modes == WorldMapSupportedGameModeFlags::FREEFORM {
        WorldSessionMode::Freeform
    } else if map.supported_game_modes == WorldMapSupportedGameModeFlags::CHALLENGE {
        WorldSessionMode::Challenge
    } else if map.supported_game_modes == WorldMapSupportedGameModeFlags::CAMPAIGN {
        WorldSessionMode::Campaign
    } else {
        return Err(invalid("saved world has no unique game mode"));
    };
    let grid = terrain.canonical_terrain_grid().clone();
    let count = u32::try_from(grid.water_regions.len())
        .map_err(|_| invalid("saved terrain water region count exceeds u32"))?;
    Ok((
        WorldSnapshotRecord {
            scenario_definition_identifier: map.id,
            requested_world_definition_identifier: map.id,
            map_definition_identifier: map.id,
            starting_point_definition_identifier: start.id,
            session_mode: mode,
            last_saved_unix_timestamp_milliseconds,
            display_name: start.name.clone(),
            source_document: Some(saved.source.clone()),
        },
        TerrainSnapshotRecord {
            grid,
            original_water_region_count: count,
            edited_water_surface_heights: Vec::new(),
            edited_chunks: Vec::new(),
            retired_water_region_rows: Vec::new(),
        },
    ))
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
