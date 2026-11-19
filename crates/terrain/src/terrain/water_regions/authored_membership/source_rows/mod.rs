use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use super::neighbouring_tiles;

/// Authored rows interact only when their points can commit to a common slope
/// tile. These groups retain source-row ordering for incremental settlement.
#[derive(Component)]
pub(in crate::terrain) struct AuthoredWaterRowGroups {
    by_row: Vec<usize>,
    rows: Vec<Vec<usize>>,
    tile_rows: HashMap<UVec2, usize>,
}

impl AuthoredWaterRowGroups {
    pub(super) fn new(asset: &TerrainAsset) -> Self {
        let grid = asset.canonical_terrain_grid();
        let mut tiles = HashMap::<UVec2, usize>::default();
        let mut parent: Vec<_> = (0..grid.water_regions.len()).collect();
        for (row, region) in grid.water_regions.iter().enumerate() {
            for &index in &region.row_major_cell_indices {
                let point = UVec2::new(index % grid.width, index / grid.width);
                for tile in neighbouring_tiles(point, (grid.width - 1) / 4, (grid.height - 1) / 4) {
                    if let Some(&neighbour) = tiles.get(&tile) {
                        let first = find(&mut parent, row);
                        let second = find(&mut parent, neighbour);
                        parent[second] = first;
                    } else {
                        tiles.insert(tile, row);
                    }
                }
            }
        }
        let mut rows = vec![Vec::new(); parent.len()];
        let by_row: Vec<_> = (0..parent.len())
            .map(|row| find(&mut parent, row))
            .collect();
        for (row, &group) in by_row.iter().enumerate() {
            rows[group].push(row);
        }
        Self {
            by_row,
            rows,
            tile_rows: tiles,
        }
    }

    pub(super) fn connecting_row(&self, tile: UVec2) -> Option<usize> {
        self.tile_rows.get(&tile).copied()
    }

    pub(super) fn affected_rows(&self, changed: &HashSet<usize>) -> Vec<usize> {
        let groups: HashSet<_> = changed
            .iter()
            .filter_map(|row| self.by_row.get(*row))
            .copied()
            .collect();
        let mut rows: Vec<_> = groups
            .into_iter()
            .flat_map(|group| self.rows[group].iter().copied())
            .collect();
        rows.sort_unstable();
        rows
    }
}

fn find(parent: &mut [usize], mut row: usize) -> usize {
    while parent[row] != row {
        parent[row] = parent[parent[row]];
        row = parent[row];
    }
    row
}
