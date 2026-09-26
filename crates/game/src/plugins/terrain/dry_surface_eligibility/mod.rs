mod terrain_height_support_eligibility;
mod unchanged_water_region_eligibility;

use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::terrain::TerrainWaterDepth;

use crate::{
    assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    assets::world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
    plugins::{
        aquatic::aquatic_simulation_types::Tank,
        topology::topology_graph_types::{FenceEdge, PathTile},
        transport_tours::transport_topology_types::TrackSegment,
        world_spawn::world_membership_types::DefinitionId,
    },
};

use super::{
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
    terrain_sample_grid_queries::{
        authored_terrain_sample_index, calculate_row_major_terrain_sample_index,
        read_authored_terrain_sample,
    },
    terrain_world_sampling::terrain_chunk_at,
};

/// Terrain, edits and tanks used to determine whether a position is dry.
#[derive(SystemParam)]
pub(crate) struct TerrainDrySurfaceEligibility<'w, 's> {
    index: Res<'w, TerrainIndex>,
    assets: Res<'w, Assets<TerrainAsset>>,
    chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
    tanks: Query<'w, 's, (), With<Tank>>,
    paths: Query<'w, 's, &'static PathTile>,
    fences: Query<'w, 's, &'static FenceEdge>,
    tracks: Query<'w, 's, &'static TrackSegment>,
    objects: Query<'w, 's, &'static DefinitionId>,
    definitions: Res<'w, Assets<WorldDefinitionAsset>>,
    active_definitions: Res<'w, WorldDefinitions>,
}

impl TerrainDrySurfaceEligibility<'_, '_> {
    pub(crate) fn on_land(&self, world: Vec2) -> Option<bool> {
        // Tank surfaces can supply links not represented by editable samples.
        // Until their side ownership is integrated, do not infer an exclusion
        // from a guessed volume or water-height intersection at the target.
        if !self.tanks.is_empty() {
            return None;
        }
        let entity = terrain_chunk_at(&self.index, world)?;
        let (chunk, edited) = self.chunks.get(entity).ok()?;
        let asset = self.assets.get(&chunk.asset)?;
        let cells = usize::from(chunk.side.checked_sub(1)?);
        if cells == 0 || cells % 4 != 0 || !chunk.spacing_m.is_finite() || chunk.spacing_m <= 0.0 {
            return None;
        }
        let local = (world - chunk.origin) / chunk.spacing_m;
        if !local.is_finite()
            || !local.cmpge(Vec2::ZERO).all()
            || !local.cmple(Vec2::splat(cells as f32)).all()
        {
            return None;
        }
        let minimum_x = (local.x.floor() as usize / 4) * 4;
        let minimum_source_z = ((cells as f32 - local.y).floor() as usize / 4) * 4;
        let minimum_z = cells.checked_sub(minimum_source_z.checked_add(4)?)?;
        if minimum_x.checked_add(4)? > cells {
            return None;
        }
        let mut all_dry = true;
        for z in minimum_z..=minimum_z + 4 {
            for x in minimum_x..=minimum_x + 4 {
                let sample = read_authored_terrain_sample(chunk, asset, x, z)?;
                // Saved region cells may acquire links during side refresh,
                // even when the sample's serialized link flag was absent.
                // Distant region cells cannot seed this tile's four sides.
                let source_index = authored_terrain_sample_index(chunk, asset, x, z)?;
                if asset.sample_has_authored_water_region(source_index)?
                    || sample.water_depth != TerrainWaterDepth::Dry
                    || sample.water_surface_flag
                    || sample.height_is_linked
                    || sample.has_child_height_link
                {
                    all_dry = false;
                }
                if let Some(edited) = edited {
                    let index = calculate_row_major_terrain_sample_index(chunk.side, x, z)?;
                    if *edited.water_styles.get(index)? != 0 {
                        all_dry = false;
                    }
                }
            }
        }
        if all_dry {
            Some(true)
        } else if edited.is_none() {
            self.on_land_in_unchanged_water_region(chunk, asset, minimum_x, minimum_z, world)
        } else {
            None
        }
    }
}
