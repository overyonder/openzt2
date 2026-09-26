use super::{
    locomotion_types::NavFlags,
    terrain_derived_navigation_graph_types::{
        TerrainDerivedNavigationGraph, TerrainDerivedNavigationGraphEdge,
        TerrainDerivedNavigationGraphEdgeFlags, TerrainDerivedNavigationGraphTraversalKind,
    },
};

impl TerrainDerivedNavigationGraph {
    pub(super) fn iterate_outgoing_navigation_graph_edges(
        &self,
        source_navigation_node: u32,
    ) -> impl Iterator<Item = TerrainDerivedNavigationGraphEdge> + '_ {
        let mut destination_navigation_nodes = [u32::MAX; 4];
        let mut destination_navigation_node_count = 0;
        if let Some(source_terrain_cell) =
            self.calculate_terrain_cell_for_navigation_node(source_navigation_node)
        {
            for candidate_terrain_cell in [
                source_terrain_cell[0]
                    .checked_sub(1)
                    .map(|x| [x, source_terrain_cell[1]]),
                source_terrain_cell[0]
                    .checked_add(1)
                    .map(|x| [x, source_terrain_cell[1]]),
                source_terrain_cell[1]
                    .checked_sub(1)
                    .map(|z| [source_terrain_cell[0], z]),
                source_terrain_cell[1]
                    .checked_add(1)
                    .map(|z| [source_terrain_cell[0], z]),
            ]
            .into_iter()
            .flatten()
            {
                if let Some(destination_navigation_node) =
                    self.calculate_navigation_node_for_terrain_cell(candidate_terrain_cell)
                {
                    destination_navigation_nodes[destination_navigation_node_count] =
                        destination_navigation_node;
                    destination_navigation_node_count += 1;
                }
            }
        }
        destination_navigation_nodes[..destination_navigation_node_count].sort_unstable();
        destination_navigation_nodes
            .into_iter()
            .take(destination_navigation_node_count)
            .filter_map(move |destination_navigation_node| {
                self.derive_navigation_graph_edge(
                    source_navigation_node,
                    destination_navigation_node,
                )
            })
    }

    fn derive_navigation_graph_edge(
        &self,
        source_navigation_node: u32,
        destination_navigation_node: u32,
    ) -> Option<TerrainDerivedNavigationGraphEdge> {
        let source_node = self.find_navigation_node(source_navigation_node)?;
        let destination_node = self.find_navigation_node(destination_navigation_node)?;
        let traverses_water = source_node.eligibility_flags.contains(NavFlags::WATER);
        if traverses_water != destination_node.eligibility_flags.contains(NavFlags::WATER) {
            return None;
        }
        let follows_path = source_node.eligibility_flags.contains(NavFlags::PATH)
            || destination_node.eligibility_flags.contains(NavFlags::PATH);
        let mut edge_flags = TerrainDerivedNavigationGraphEdgeFlags::default();
        if source_node
            .eligibility_flags
            .contains(NavFlags::DISABLED_BY_DEFAULT)
            || destination_node
                .eligibility_flags
                .contains(NavFlags::DISABLED_BY_DEFAULT)
        {
            edge_flags.0 |= TerrainDerivedNavigationGraphEdgeFlags::DISABLED_BY_DEFAULT.0;
        }
        if follows_path {
            edge_flags.0 |= TerrainDerivedNavigationGraphEdgeFlags::PREFERRED.0;
        }
        Some(TerrainDerivedNavigationGraphEdge {
            destination_node: destination_navigation_node,
            traversal_cost_millimetres: calculate_distance_millimetres_between_centimetre_positions(
                self.calculate_navigation_node_world_position_centimetres(source_navigation_node)?,
                self.calculate_navigation_node_world_position_centimetres(
                    destination_navigation_node,
                )?,
            ),
            width_centimetres: source_node
                .clearance_centimetres
                .min(destination_node.clearance_centimetres),
            traversal_kind: if traverses_water {
                TerrainDerivedNavigationGraphTraversalKind::Swim
            } else if follows_path {
                TerrainDerivedNavigationGraphTraversalKind::Path
            } else {
                TerrainDerivedNavigationGraphTraversalKind::Ground
            },
            edge_flags,
        })
    }
}

fn calculate_distance_millimetres_between_centimetre_positions(
    first_position_centimetres: [i32; 3],
    second_position_centimetres: [i32; 3],
) -> u32 {
    let squared_distance_in_square_centimetres = first_position_centimetres
        .into_iter()
        .zip(second_position_centimetres)
        .map(|(first_coordinate, second_coordinate)| {
            let coordinate_difference =
                i128::from(first_coordinate) - i128::from(second_coordinate);
            let coordinate_magnitude = coordinate_difference.unsigned_abs();
            coordinate_magnitude * coordinate_magnitude
        })
        .sum::<u128>();
    u32::try_from(
        squared_distance_in_square_centimetres
            .isqrt()
            .saturating_mul(10),
    )
    .unwrap_or(u32::MAX)
    .max(1)
}
