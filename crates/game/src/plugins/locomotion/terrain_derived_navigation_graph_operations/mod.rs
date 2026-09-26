use bevy::math::UVec2;

use super::{
    locomotion_types::NavFlags,
    terrain_derived_navigation_graph_types::{
        TerrainDerivedNavigationGraph, TerrainDerivedNavigationGraphNode,
    },
};

impl TerrainDerivedNavigationGraph {
    pub(super) fn navigation_node_count(&self) -> usize {
        self.navigation_nodes.len()
    }

    pub(super) fn find_navigation_node(
        &self,
        navigation_node: u32,
    ) -> Option<&TerrainDerivedNavigationGraphNode> {
        self.navigation_nodes.get(navigation_node as usize)
    }

    pub(super) fn update_navigation_node_from_changed_terrain_cell(
        &mut self,
        terrain_cell: UVec2,
        height_centimetres: i32,
        contains_water: bool,
    ) -> Option<()> {
        let node = self
            .calculate_navigation_node_for_terrain_cell(terrain_cell.to_array())
            .and_then(|index| self.navigation_nodes.get_mut(index as usize))?;
        node.height_centimetres = height_centimetres;
        if contains_water {
            node.eligibility_flags.0 |= NavFlags::WATER.0;
        } else {
            node.eligibility_flags.0 &= !NavFlags::WATER.0;
        }
        Some(())
    }

    pub(super) const fn terrain_tile_size_centimetres(&self) -> u32 {
        self.terrain_tile_size_centimetres
    }

    pub(super) const fn terrain_tile_grid_dimensions(&self) -> [u32; 2] {
        self.terrain_tile_grid_dimensions
    }

    pub(super) fn find_candidate_node_indices_in_containing_terrain_tile(
        &self,
        position_centimetres: [i32; 3],
    ) -> std::ops::Range<u32> {
        let terrain_tile_size_centimetres =
            i64::from(self.terrain_cell_size_centimetres) * i64::from(self.terrain_cells_per_tile);
        let x = i64::from(position_centimetres[0]);
        let z = i64::from(position_centimetres[2]);
        if x < 0 || z > 0 || terrain_tile_size_centimetres == 0 {
            return 0..0;
        }
        let coordinate = [
            u32::try_from(x / terrain_tile_size_centimetres).ok(),
            u32::try_from(-z / terrain_tile_size_centimetres).ok(),
        ];
        let [Some(x), Some(z)] = coordinate else {
            return 0..0;
        };
        self.calculate_terrain_tile_node_index_range([x, z])
            .unwrap_or(0..0)
    }

    pub(super) fn calculate_navigation_node_world_position_centimetres(
        &self,
        navigation_node: u32,
    ) -> Option<[i32; 3]> {
        let terrain_cell = self.calculate_terrain_cell_for_navigation_node(navigation_node)?;
        let height_centimetres = self
            .find_navigation_node(navigation_node)?
            .height_centimetres;
        let size = i64::from(self.terrain_cell_size_centimetres);
        let coordinate = |value: u32| {
            i64::from(value)
                .checked_mul(size)?
                .checked_add(size / 2)
                .and_then(|value| i32::try_from(value).ok())
        };
        Some([
            coordinate(terrain_cell[0])?,
            height_centimetres,
            coordinate(terrain_cell[1])?.checked_neg()?,
        ])
    }

    fn calculate_terrain_tile_node_index_range(
        &self,
        terrain_tile_coordinate: [u32; 2],
    ) -> Option<std::ops::Range<u32>> {
        let terrain_tile_linear_index =
            self.calculate_terrain_tile_linear_index(terrain_tile_coordinate)?;
        let navigation_nodes_per_tile = u32::from(self.terrain_cells_per_tile).checked_pow(2)?;
        let first_navigation_node = u32::try_from(terrain_tile_linear_index)
            .ok()?
            .checked_mul(navigation_nodes_per_tile)?;
        Some(first_navigation_node..first_navigation_node.checked_add(navigation_nodes_per_tile)?)
    }

    fn calculate_terrain_tile_linear_index(
        &self,
        terrain_tile_coordinate: [u32; 2],
    ) -> Option<usize> {
        (terrain_tile_coordinate[0] < self.terrain_tile_grid_dimensions[0]
            && terrain_tile_coordinate[1] < self.terrain_tile_grid_dimensions[1])
            .then(|| {
                usize::try_from(
                    u64::from(terrain_tile_coordinate[1])
                        * u64::from(self.terrain_tile_grid_dimensions[0])
                        + u64::from(terrain_tile_coordinate[0]),
                )
                .ok()
            })
            .flatten()
    }

    pub(super) fn calculate_terrain_cell_for_navigation_node(
        &self,
        navigation_node: u32,
    ) -> Option<[u32; 2]> {
        let terrain_cells_per_tile = u32::from(self.terrain_cells_per_tile);
        let navigation_nodes_per_tile = terrain_cells_per_tile.checked_pow(2)?;
        ((navigation_node as usize) < self.navigation_nodes.len()).then_some(())?;
        let terrain_tile_linear_index = navigation_node / navigation_nodes_per_tile;
        let local_navigation_node = navigation_node % navigation_nodes_per_tile;
        Some([
            (terrain_tile_linear_index % self.terrain_tile_grid_dimensions[0])
                .checked_mul(terrain_cells_per_tile)?
                .checked_add(local_navigation_node / terrain_cells_per_tile)?,
            (terrain_tile_linear_index / self.terrain_tile_grid_dimensions[0])
                .checked_mul(terrain_cells_per_tile)?
                .checked_add(local_navigation_node % terrain_cells_per_tile)?,
        ])
    }

    pub(super) fn calculate_navigation_node_for_terrain_cell(
        &self,
        terrain_cell: [u32; 2],
    ) -> Option<u32> {
        let terrain_cells_per_tile = u32::from(self.terrain_cells_per_tile);
        if terrain_cell[0]
            >= self.terrain_tile_grid_dimensions[0].checked_mul(terrain_cells_per_tile)?
            || terrain_cell[1]
                >= self.terrain_tile_grid_dimensions[1].checked_mul(terrain_cells_per_tile)?
        {
            return None;
        }
        let terrain_tile_coordinate = [
            terrain_cell[0] / terrain_cells_per_tile,
            terrain_cell[1] / terrain_cells_per_tile,
        ];
        let local_terrain_cell = [
            terrain_cell[0] % terrain_cells_per_tile,
            terrain_cell[1] % terrain_cells_per_tile,
        ];
        terrain_tile_coordinate[1]
            .checked_mul(self.terrain_tile_grid_dimensions[0])?
            .checked_add(terrain_tile_coordinate[0])?
            .checked_mul(terrain_cells_per_tile.checked_pow(2)?)?
            .checked_add(local_terrain_cell[0].checked_mul(terrain_cells_per_tile)?)?
            .checked_add(local_terrain_cell[1])
    }
}
