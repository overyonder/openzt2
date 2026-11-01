use openzt2_game_data::terrain::TerrainGrid;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct TerrainSnapshotRecord {
    pub grid: TerrainGrid,
    pub original_water_region_count: u32,
    /// Exact absolute planes for live edits; relative heights can lose precision.
    pub edited_water_surface_heights: Vec<(u32, f32)>,
    pub edited_chunks: Vec<TerrainEditedChunkSnapshotRecord>,
    pub retired_water_region_rows: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TerrainEditedChunkSnapshotRecord {
    pub source_coord: [i32; 2],
    pub side: u16,
    pub samples: EditedTerrainSamples,
    /// These saved SourceRow references restore as Edited owners, bypassing source attachment selection.
    pub edited_water_owner_samples: Vec<u32>,
}
