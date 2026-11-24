use bevy::math::UVec2;

use crate::aquatic::tank_navigation_water::TankNavigationWater;

use crate::locomotion::locomotion_types::NavFlags;
use crate::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph;
use crate::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraphNode;

impl TerrainDerivedNavigationGraph {
    pub fn navigation_node_count(&self) -> usize {
        self.navigation_nodes.len() + self.path_surfaces.len()
    }

    pub fn find_navigation_node(
        &self,
        navigation_node: u32,
    ) -> Option<&TerrainDerivedNavigationGraphNode> {
        self.navigation_nodes
            .get(navigation_node as usize)
            .or_else(|| {
                self.path_surface_at_node(navigation_node)
                    .map(|surface| &surface.node)
            })
    }

    pub fn effective_navigation_node(
        &self,
        index: u32,
        tank_water: Option<&TankNavigationWater>,
    ) -> Option<TerrainDerivedNavigationGraphNode> {
        let mut node = *self.find_navigation_node(index)?;
        if let Some(depth) = tank_water.and_then(|water| water.depth_centimetres(index)) {
            node.eligibility_flags.0 |= NavFlags::WATER.0;
            node.water_depth_centimetres = depth;
        }
        Some(node)
    }

    pub fn find_navigation_node_mut(
        &mut self,
        navigation_node: u32,
    ) -> Option<&mut TerrainDerivedNavigationGraphNode> {
        self.navigation_nodes.get_mut(navigation_node as usize)
    }

    pub fn update_navigation_node_from_changed_terrain_cell(
        &mut self,
        terrain_cell: UVec2,
        height_centimetres: i32,
        contains_water: bool,
        minimum_triangle_normal_y: f32,
    ) -> Option<()> {
        let node = self
            .calculate_navigation_node_for_terrain_cell(terrain_cell.to_array())
            .and_then(|index| self.navigation_nodes.get_mut(index as usize))?;
        node.height_centimetres = height_centimetres;
        node.minimum_triangle_normal_y = minimum_triangle_normal_y;
        if contains_water {
            node.eligibility_flags.0 |= NavFlags::WATER.0;
        } else {
            node.eligibility_flags.0 &= !NavFlags::WATER.0;
        }
        Some(())
    }

    pub const fn terrain_tile_size_centimetres(&self) -> u32 {
        self.terrain_tile_size_centimetres
    }

    pub const fn terrain_tile_grid_dimensions(&self) -> [u32; 2] {
        self.terrain_tile_grid_dimensions
    }

    pub fn find_candidate_node_indices_in_containing_terrain_tile(
        &self,
        position_centimetres: [i32; 3],
    ) -> impl Iterator<Item = u32> + '_ {
        let terrain = self.find_terrain_node_indices_in_containing_tile(position_centimetres);
        let elevated = terrain
            .clone()
            .filter_map(move |node| self.calculate_terrain_cell_for_navigation_node(node))
            .flat_map(move |cell| {
                self.path_surfaces_by_cell
                    .get(&cell)
                    .into_iter()
                    .flat_map(std::collections::BTreeMap::values)
                    .copied()
                    .filter(move |node| {
                        self.calculate_terrain_cell_for_navigation_node(*node) == Some(cell)
                    })
            });
        terrain.chain(elevated)
    }

    fn find_terrain_node_indices_in_containing_tile(
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

    pub fn calculate_navigation_node_world_position_centimetres(
        &self,
        navigation_node: u32,
    ) -> Option<[i32; 3]> {
        if let Some(surface) = self.path_surface_at_node(navigation_node) {
            let position = surface.centre;
            return Some(
                position
                    .to_array()
                    .map(|value| (value * 100.0).round() as i32),
            );
        }
        let [x, z] = *self.terrain_centres_xz.get(navigation_node as usize)?;
        if x < 0 || z > 0 {
            return None;
        }
        Some([
            x,
            self.navigation_nodes[navigation_node as usize].height_centimetres,
            z,
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

    pub fn calculate_terrain_cell_for_navigation_node(
        &self,
        navigation_node: u32,
    ) -> Option<[u32; 2]> {
        if let Some(surface) = self.path_surface_at_node(navigation_node) {
            return self.terrain_cell_at_world_position(surface.centre);
        }
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

    pub fn calculate_navigation_node_for_terrain_cell(
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
