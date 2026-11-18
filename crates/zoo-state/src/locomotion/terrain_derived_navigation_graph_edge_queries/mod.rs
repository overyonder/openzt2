use crate::locomotion::locomotion_types::NavFlags;
use crate::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph;
use crate::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraphEdge;
use crate::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraphEdgeFlags;
use crate::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraphTraversalKind;

impl TerrainDerivedNavigationGraph {
    pub fn iterate_outgoing_navigation_graph_edges(
        &self,
        source_navigation_node: u32,
    ) -> impl Iterator<Item = TerrainDerivedNavigationGraphEdge> + '_ {
        self.terrain_neighbors
            .get(source_navigation_node as usize)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&next| next != u32::MAX)
            .filter_map(move |next| self.derive_navigation_graph_edge(source_navigation_node, next))
            .chain(
                self.path_surface_edges
                    .get(&source_navigation_node)
                    .into_iter()
                    .flatten()
                    .copied(),
            )
    }

    pub(super) fn derive_navigation_graph_edge(
        &self,
        source_navigation_node: u32,
        destination_navigation_node: u32,
    ) -> Option<TerrainDerivedNavigationGraphEdge> {
        let source_node = self.find_navigation_node(source_navigation_node)?;
        let destination_node = self.find_navigation_node(destination_navigation_node)?;
        let traverses_water = source_node.eligibility_flags.contains(NavFlags::WATER);
        let crosses_shore =
            traverses_water != destination_node.eligibility_flags.contains(NavFlags::WATER);
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
            traversal_cost_millimetres: if source_navigation_node
                < self.navigation_nodes.len() as u32
                && destination_navigation_node < self.navigation_nodes.len() as u32
            {
                let horizontal = u64::from(self.terrain_cell_size_centimetres);
                let rise = u64::from(
                    source_node
                        .height_centimetres
                        .abs_diff(destination_node.height_centimetres),
                );
                let length = if rise == 0 {
                    horizontal
                } else {
                    (horizontal * horizontal + rise * rise).isqrt()
                };
                u32::try_from(length * 10).unwrap_or(u32::MAX)
            } else {
                calculate_distance_millimetres_between_centimetre_positions(
                    self.calculate_navigation_node_world_position_centimetres(
                        source_navigation_node,
                    )?,
                    self.calculate_navigation_node_world_position_centimetres(
                        destination_navigation_node,
                    )?,
                )
            },
            // Clearance is a radius, so the narrower cell leaves twice its clearance.
            width_centimetres: source_node
                .clearance_centimetres
                .min(destination_node.clearance_centimetres)
                .saturating_mul(2),
            traversal_kind: if crosses_shore {
                TerrainDerivedNavigationGraphTraversalKind::Shore
            } else if traverses_water {
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
