use bevy::{platform::collections::HashMap, prelude::*};
use openzt2_game_data::AssetId;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::AUTHORED_GROUND_PATH_SURFACE_MASK_SOURCE_PATHS;
use crate::plugins::topology::ground_path_layout_calculation::calculate_snapped_ground_path_layout;
use crate::plugins::topology::topology_graph_types::EdgeKey;

#[derive(Clone, Copy)]
pub(super) struct GroundPathSurfaceMaskTextureSelection {
    pub(super) cell: IVec3,
    pub(super) source_path: &'static str,
    pub(super) mirror_mode: u8,
}

pub(super) fn select_authored_ground_path_surface_mask_textures(
    direction: u8,
    centre: IVec3,
    width_cells: i32,
    curb: AssetId,
    curb_edges: &HashMap<(AssetId, EdgeKey), (IVec3, IVec3)>,
) -> Option<[Option<GroundPathSurfaceMaskTextureSelection>; 2]> {
    let corner_offsets = match direction {
        0 => (
            IVec3::new(-width_cells, width_cells, 0),
            IVec3::new(width_cells, width_cells, 0),
        ),
        2 => (
            IVec3::new(width_cells, width_cells, 0),
            IVec3::new(width_cells, -width_cells, 0),
        ),
        4 => (
            IVec3::new(width_cells, -width_cells, 0),
            IVec3::new(-width_cells, -width_cells, 0),
        ),
        6 => (
            IVec3::new(-width_cells, -width_cells, 0),
            IVec3::new(-width_cells, width_cells, 0),
        ),
        _ => return None,
    };
    let Some(boundary) = EdgeKey::new(centre + corner_offsets.0, centre + corner_offsets.1) else {
        return None;
    };
    let direction_cell = centre
        + match direction {
            0 => IVec3::new(0, width_cells / 2, 0),
            2 => IVec3::new(width_cells / 2, 0, 0),
            4 => IVec3::new(0, -width_cells / 2, 0),
            6 => IVec3::new(-width_cells / 2, 0, 0),
            _ => IVec3::ZERO,
        };
    if let Some((first_endpoint, _)) = curb_edges.get(&(curb, boundary)) {
        let vector_index = classify_authored_active_fence_endpoint_vector_index(
            (*first_endpoint - direction_cell).xy(),
            width_cells / 2,
        );
        return Some(
            vector_index
                .and_then(|vector_index| {
                    select_authored_active_fence_ground_path_surface_masks(
                        direction,
                        vector_index,
                        direction_cell,
                        width_cells,
                    )
                })
                .unwrap_or_else(|| {
                    single_ground_path_surface_mask_texture_selection(
                        direction_cell,
                        unconnected_ground_path_surface_mask(direction)
                            .expect("validated cardinal ground-path direction"),
                    )
                }),
        );
    }
    let Some((first_endpoint, second_endpoint)) =
        resolve_authored_neighbor_projection_curb_edge_by_orientation(
            direction,
            centre,
            direction_cell,
            corner_offsets,
            curb,
            boundary,
            curb_edges,
        )
    else {
        return Some(single_ground_path_surface_mask_texture_selection(
            direction_cell,
            unconnected_ground_path_surface_mask(direction)?,
        ));
    };
    let direction_pair_code = classify_authored_fence_direction_pair_code(
        (first_endpoint - centre).xy().signum(),
        (second_endpoint - centre).xy().signum(),
    );
    let selected = direction_pair_code
        .and_then(|code| numbered_authored_ground_path_surface_mask(direction, code))
        .or_else(|| unconnected_ground_path_surface_mask(direction))?;
    Some(single_ground_path_surface_mask_texture_selection(
        direction_cell,
        selected,
    ))
}

fn resolve_authored_neighbor_projection_curb_edge_by_orientation(
    direction: u8,
    centre: IVec3,
    direction_cell: IVec3,
    corner_offsets: (IVec3, IVec3),
    curb: AssetId,
    boundary: EdgeKey,
    curb_edges: &HashMap<(AssetId, EdgeKey), (IVec3, IVec3)>,
) -> Option<(IVec3, IVec3)> {
    let probes = match direction {
        0 => [
            (corner_offsets.0, IVec2::new(-1, 1)),
            (corner_offsets.1, IVec2::new(1, 1)),
        ],
        2 => [
            (corner_offsets.0, IVec2::new(1, 1)),
            (corner_offsets.1, IVec2::new(1, -1)),
        ],
        4 => [
            (corner_offsets.1, IVec2::new(-1, -1)),
            (corner_offsets.0, IVec2::new(1, -1)),
        ],
        6 => [
            (corner_offsets.1, IVec2::new(-1, 1)),
            (corner_offsets.0, IVec2::new(-1, -1)),
        ],
        _ => return None,
    };
    probes
        .into_iter()
        .find_map(|(corner_offset, quadrant)| {
            let corner = centre + corner_offset;
            curb_edges
                .iter()
                .find_map(|((definition, key), endpoints)| {
                    if *definition != curb || *key == boundary {
                        return None;
                    }
                    let (first, second) = *endpoints;
                    let touches_probe_corner = first == corner || second == corner;
                    let doubled_midpoint_offset =
                        (first + second - direction_cell * 2).xy().signum();
                    (touches_probe_corner && doubled_midpoint_offset == quadrant)
                        .then_some(endpoints)
                })
        })
        .copied()
}

fn classify_authored_active_fence_endpoint_vector_index(
    endpoint_offset: IVec2,
    direction_cell_offset: i32,
) -> Option<u8> {
    if direction_cell_offset == 0
        || endpoint_offset.x % direction_cell_offset != 0
        || endpoint_offset.y % direction_cell_offset != 0
    {
        return None;
    }
    match endpoint_offset / direction_cell_offset {
        IVec2 { x: -1, y: 2 } => Some(0),
        IVec2 { x: -1, y: 1 } => Some(1),
        IVec2 { x: -1, y: -1 } => Some(2),
        IVec2 { x: -1, y: -2 } => Some(3),
        IVec2 { x: 1, y: 2 } => Some(4),
        IVec2 { x: 1, y: 1 } => Some(5),
        IVec2 { x: 1, y: -1 } => Some(6),
        IVec2 { x: 1, y: -2 } => Some(7),
        IVec2 { x: -2, y: 1 } => Some(8),
        IVec2 { x: -2, y: -1 } => Some(9),
        IVec2 { x: 2, y: 1 } => Some(10),
        IVec2 { x: 2, y: -1 } => Some(11),
        _ => None,
    }
}

fn select_authored_active_fence_ground_path_surface_masks(
    direction: u8,
    vector_index: u8,
    direction_cell: IVec3,
    width_cells: i32,
) -> Option<[Option<GroundPathSurfaceMaskTextureSelection>; 2]> {
    let (first, second) = match (direction, vector_index) {
        (0, 1) => ((0, 0), Some((1, 0))),
        (0, 2) => ((2, 0), None),
        (0, 5) => ((0, 0), Some((1, 1))),
        (0, 6) => ((2, 1), None),
        (0, 8) => ((0, 0), Some((3, 0))),
        (0, 9) => ((4, 0), None),
        (0, 10) => ((0, 0), Some((3, 1))),
        (0, 11) => ((4, 1), None),
        (2, 0) => ((13, 3), None),
        (2, 1) => ((12, 3), None),
        (2, 2) => ((12, 1), None),
        (2, 3) => ((13, 1), None),
        (2, 4) => ((9, 1), Some((11, 3))),
        (2, 5) => ((9, 1), Some((10, 3))),
        (2, 6) => ((9, 1), Some((10, 1))),
        (2, 7) => ((9, 1), Some((11, 1))),
        (4, 1) => ((2, 1), None),
        (4, 2) => ((0, 2), Some((1, 2))),
        (4, 5) => ((2, 3), None),
        (4, 6) => ((0, 2), Some((1, 3))),
        (4, 8) => ((4, 1), None),
        (4, 9) => ((0, 2), Some((3, 2))),
        (4, 10) => ((4, 3), None),
        (4, 11) => ((0, 2), Some((3, 3))),
        (6, 0) => ((9, 0), Some((11, 2))),
        (6, 1) => ((9, 0), Some((10, 2))),
        (6, 2) => ((9, 0), Some((10, 0))),
        (6, 3) => ((9, 0), Some((11, 0))),
        (6, 4) => ((13, 2), None),
        (6, 5) => ((12, 2), None),
        (6, 6) => ((12, 0), None),
        (6, 7) => ((13, 0), None),
        _ => return None,
    };
    let first = GroundPathSurfaceMaskTextureSelection {
        cell: direction_cell,
        source_path: AUTHORED_GROUND_PATH_SURFACE_MASK_SOURCE_PATHS[first.0],
        mirror_mode: first.1,
    };
    let second =
        second.map(
            |(source_path_index, mirror_mode)| GroundPathSurfaceMaskTextureSelection {
                cell: advance_ground_path_surface_direction_cell(
                    direction_cell,
                    direction,
                    width_cells,
                ),
                source_path: AUTHORED_GROUND_PATH_SURFACE_MASK_SOURCE_PATHS[source_path_index],
                mirror_mode,
            },
        );
    Some([Some(first), second])
}

fn advance_ground_path_surface_direction_cell(
    cell: IVec3,
    direction: u8,
    width_cells: i32,
) -> IVec3 {
    cell + match direction {
        0 => IVec3::new(0, width_cells, 0),
        2 => IVec3::new(width_cells, 0, 0),
        4 => IVec3::new(0, -width_cells, 0),
        6 => IVec3::new(-width_cells, 0, 0),
        _ => IVec3::ZERO,
    }
}

/// Native surface records use the square's grid corner, not the directional
/// decal centre. All four directions share it; an advanced direction belongs
/// to the adjacent square. Return the source corner in canonical Bevy XZ cells.
pub(super) fn ground_path_surface_origin_from_source_direction_cell(
    source_cell: IVec3,
    width_centimetres: u16,
    topology_cell_centimetres: u16,
) -> Option<IVec3> {
    let (centre, width_cells, _) = calculate_snapped_ground_path_layout(
        source_cell,
        width_centimetres,
        topology_cell_centimetres,
    )?;
    Some(IVec3::new(
        centre.x - width_cells,
        -(centre.y - width_cells),
        centre.z,
    ))
}

fn single_ground_path_surface_mask_texture_selection(
    cell: IVec3,
    selected: (&'static str, u8),
) -> [Option<GroundPathSurfaceMaskTextureSelection>; 2] {
    [
        Some(GroundPathSurfaceMaskTextureSelection {
            cell,
            source_path: selected.0,
            mirror_mode: selected.1,
        }),
        None,
    ]
}

fn classify_authored_fence_direction_pair_code(first: IVec2, second: IVec2) -> Option<u8> {
    match (first.x, first.y, second.x, second.y) {
        (1, 1, 1, -1) => Some(13),
        (1, 1, -1, 1) => Some(15),
        (1, 1, 0, 1) => Some(23),
        (1, 1, 1, 0) => Some(26),
        (1, -1, 1, 1) => Some(14),
        (1, -1, -1, -1) => Some(16),
        (1, -1, 1, 0) => Some(21),
        (1, -1, 0, -1) => Some(24),
        (-1, 1, -1, -1) => Some(17),
        (-1, 1, 1, 1) => Some(19),
        (-1, 1, -1, 0) => Some(25),
        (-1, 1, 0, 1) => Some(28),
        (-1, -1, -1, 1) => Some(18),
        (-1, -1, 1, -1) => Some(20),
        (-1, -1, -1, 0) => Some(22),
        (-1, -1, 0, -1) => Some(27),
        _ => None,
    }
}

fn numbered_authored_ground_path_surface_mask(
    direction: u8,
    code: u8,
) -> Option<(&'static str, u8)> {
    let (index, mirror) = match (direction, code) {
        (0, 23) => (7, 0),
        (0, 13) => (5, 0),
        (0, 15) => (6, 0),
        (0, 17) => (5, 1),
        (0, 19) => (6, 1),
        (0, 25 | 26) => (8, 0),
        (0, 28) => (7, 1),
        (2, 21) => (16, 3),
        (2, 13) => (14, 1),
        (2, 14) => (14, 3),
        (2, 15) => (15, 1),
        (2, 16) => (15, 3),
        (2, 23) => (17, 1),
        (2, 24) => (17, 3),
        (2, 26) => (16, 1),
        (4, 21) => (8, 2),
        (4, 14) => (5, 2),
        (4, 16) => (6, 2),
        (4, 18) => (5, 3),
        (4, 20) => (6, 3),
        (4, 22) => (8, 3),
        (4, 24) => (7, 2),
        (4, 27) => (7, 3),
        (6, 22) => (16, 2),
        (6, 17) => (14, 0),
        (6, 18) => (14, 2),
        (6, 19) => (15, 0),
        (6, 20) => (15, 2),
        (6, 25) => (16, 0),
        (6, 27) => (17, 2),
        (6, 28) => (17, 0),
        _ => return None,
    };
    Some((
        AUTHORED_GROUND_PATH_SURFACE_MASK_SOURCE_PATHS[index],
        mirror,
    ))
}

fn unconnected_ground_path_surface_mask(direction: u8) -> Option<(&'static str, u8)> {
    let selected = match direction {
        0 => (0, 0),
        2 => (9, 1),
        4 => (0, 2),
        6 => (9, 0),
        _ => return None,
    };
    Some((
        AUTHORED_GROUND_PATH_SURFACE_MASK_SOURCE_PATHS[selected.0],
        selected.1,
    ))
}

#[cfg(test)]
mod ground_path_mask_origin_tests {
    use super::*;

    #[test]
    fn four_shipped_direction_centres_share_the_source_square_corner() {
        // Grassland concrete path centres (4.5,155.25), (5.25,154.5),
        // (4.5,153.75), (3.75,154.5), in quarter-metre topology cells.
        for cell in [
            IVec3::new(18, 621, 0),
            IVec3::new(21, 618, 0),
            IVec3::new(18, 615, 0),
            IVec3::new(15, 618, 0),
        ] {
            assert_eq!(
                ground_path_surface_origin_from_source_direction_cell(cell, 150, 25),
                Some(IVec3::new(12, -612, 0)),
            );
        }
        assert_eq!(
            ground_path_surface_origin_from_source_direction_cell(
                advance_ground_path_surface_direction_cell(IVec3::new(18, 621, 0), 0, 6),
                150,
                25,
            ),
            Some(IVec3::new(12, -624, 0)),
        );
    }
}
