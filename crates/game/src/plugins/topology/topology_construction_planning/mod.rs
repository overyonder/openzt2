use bevy::{platform::collections::HashSet, prelude::*};
use openzt2_game_data::{
    world_definitions::{
        paths_and_tile_surfaces::GuestPathDefinition, world_objects::WorldObjectKind,
    },
    AssetId,
};

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::construction::construction_interaction_types::PlacementFailure;
use crate::plugins::economy::money_types::Money;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_chunk_types::TerrainIndex;
use crate::plugins::terrain::terrain_world_sampling::sample_terrain;
use crate::plugins::terrain::terrain_world_sampling::terrain_chunk_at;

use super::{
    gate_operation_types::Gate,
    ground_path_layout_calculation::calculate_snapped_ground_path_layout,
    topology_construction_preview_types::{FencePreview, PathPreview},
    topology_graph_types::{EdgeKey, TopologyGrid, TopologyIndex},
    topology_grid_geometry::{
        calculate_adjacent_topology_grid_cells_between_endpoints,
        calculate_heading_quantized_fence_segment_cells_between_endpoints,
    },
};

pub(super) fn calculate_fence_construction_plan(
    preview: &FencePreview,
    index: &TopologyIndex,
    grid: TopologyGrid,
    definitions: WorldDefinitionsView<'_>,
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
) -> Result<(Vec<IVec3>, Money, Option<Gate>), PlacementFailure> {
    let (unit_price, gate, segment_length_cm) =
        read_fence_construction_facts(definitions, preview.definition)
            .ok_or(PlacementFailure::InvalidTopology)?;
    let spacing_cm = (grid.spacing_m * 100.0).round();
    if !spacing_cm.is_finite()
        || spacing_cm <= 0.0
        || (grid.spacing_m * 100.0 - spacing_cm).abs() > 0.001
    {
        return Err(PlacementFailure::InvalidTopology);
    }
    let spacing_cm = spacing_cm as u16;
    if segment_length_cm % spacing_cm != 0 {
        return Err(PlacementFailure::InvalidTopology);
    }
    let segment_cell_span = segment_length_cm / spacing_cm;
    let cells = calculate_heading_quantized_fence_segment_cells_between_endpoints(
        preview.from,
        preview.to,
        segment_cell_span,
    )?;
    if cells.len() < 2 {
        return Err(PlacementFailure::InvalidTopology);
    }
    let route_leaves_terrain = cells.windows(2).any(|pair| {
        calculate_adjacent_topology_grid_cells_between_endpoints(pair[0], pair[1])
            .map(|fine_cells| {
                fine_cells.into_iter().any(|cell| {
                    sample_terrain_height_at_topology_cell(
                        cell,
                        grid,
                        terrain_index,
                        terrain_assets,
                        terrain_chunks,
                    )
                    .is_none()
                })
            })
            .unwrap_or(true)
    });
    if route_leaves_terrain {
        return Err(PlacementFailure::OutsideMap);
    }
    let proposed_steps = cells
        .windows(2)
        .flat_map(|pair| {
            calculate_adjacent_topology_grid_cells_between_endpoints(pair[0], pair[1])
                .unwrap_or_default()
                .windows(2)
                .filter_map(|step| EdgeKey::new(step[0], step[1]))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let occupied = index.edges.keys().any(|edge| {
        calculate_adjacent_topology_grid_cells_between_endpoints(edge.lo, edge.hi).is_ok_and(
            |cells| {
                cells.windows(2).any(|step| {
                    EdgeKey::new(step[0], step[1]).is_some_and(|key| proposed_steps.contains(&key))
                })
            },
        )
    });
    if occupied {
        return Err(PlacementFailure::Occupied);
    }
    let segments = i64::try_from(cells.len() - 1).map_err(|_| PlacementFailure::InvalidTopology)?;
    let cost = unit_price
        .checked_mul(segments)
        .ok_or(PlacementFailure::InvalidTopology)?;
    Ok((cells, Money(cost), gate))
}

pub(super) fn calculate_path_construction_plan(
    preview: &PathPreview,
    index: &TopologyIndex,
    grid: TopologyGrid,
    definitions: WorldDefinitionsView<'_>,
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
) -> Result<(Vec<IVec3>, Money), PlacementFailure> {
    let definition = definitions
        .find_path(preview.definition)
        .ok_or(PlacementFailure::InvalidTopology)?;
    let topology_cell_size_cm = definitions.topology_cell_size_cm();
    let cells = if definition.elevated {
        calculate_adjacent_topology_grid_cells_between_endpoints(preview.from, preview.to)?
    } else {
        calculate_ground_path_construction_cells(
            preview.from,
            preview.to,
            definition.width_cm,
            topology_cell_size_cm,
            index,
        )?
    };
    let unit_price = definitions
        .find_object(AssetId(definition.object.0))
        .map(|object| object.price_cents)
        .ok_or(PlacementFailure::InvalidTopology)?;
    let terrain_heights = cells
        .iter()
        .map(|cell| {
            sample_terrain_height_at_topology_cell(
                *cell,
                grid,
                terrain_index,
                terrain_assets,
                terrain_chunks,
            )
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(PlacementFailure::OutsideMap)?;
    let (cells, terrain_heights): (Vec<_>, Vec<_>) = cells
        .into_iter()
        .zip(terrain_heights)
        .filter(|(cell, _)| !index.paths.contains_key(cell))
        .unzip();
    if cells.is_empty() {
        return Err(PlacementFailure::Occupied);
    }
    if definition.elevated || cells.iter().any(|cell| cell.z != 0) {
        let required_headroom_cm = u32::from(definition.support_headroom_cm);
        if cells.iter().zip(&terrain_heights).any(|(cell, ground)| {
            (((cell.z as f32 - *ground) * 100.0).max(0.0) as u32) < required_headroom_cm
        }) {
            return Err(PlacementFailure::NoHeadroom);
        }
        for (pair, ground) in cells.windows(2).zip(terrain_heights.windows(2)) {
            let headroom_cm = ((pair[0].z as f32 - ground[0]).min(pair[1].z as f32 - ground[1])
                * 100.0)
                .max(0.0) as u32;
            if !path_support_is_eligible(definition, pair[0], pair[1], headroom_cm) {
                return Err(PlacementFailure::NoHeadroom);
            }
        }
    }
    let count = i64::try_from(cells.len()).map_err(|_| PlacementFailure::InvalidTopology)?;
    let cost = unit_price
        .checked_mul(count)
        .ok_or(PlacementFailure::InvalidTopology)?;
    Ok((cells, Money(cost)))
}

fn calculate_ground_path_construction_cells(
    from: IVec3,
    to: IVec3,
    width_cm: u16,
    cell_size_cm: u16,
    index: &TopologyIndex,
) -> Result<Vec<IVec3>, PlacementFailure> {
    let (from, width, step) = calculate_snapped_ground_path_layout(from, width_cm, cell_size_cm)
        .ok_or(PlacementFailure::InvalidTopology)?;
    let (to, _, _) = calculate_snapped_ground_path_layout(to, width_cm, cell_size_cm)
        .ok_or(PlacementFailure::InvalidTopology)?;
    if from.z != to.z {
        return Err(PlacementFailure::NoHeadroom);
    }
    let offset_cell = |cell: IVec3, dx: i64, dy: i64| -> Result<IVec3, PlacementFailure> {
        Ok(IVec3::new(
            i32::try_from(i64::from(cell.x) + dx).map_err(|_| PlacementFailure::InvalidTopology)?,
            i32::try_from(i64::from(cell.y) + dy).map_err(|_| PlacementFailure::InvalidTopology)?,
            cell.z,
        ))
    };
    let delta_x = (i64::from(to.x) - i64::from(from.x)) / i64::from(step);
    let delta_y = (i64::from(to.y) - i64::from(from.y)) / i64::from(step);
    let diagonal_steps = delta_x.abs().min(delta_y.abs());
    let axis_steps = delta_x.abs().max(delta_y.abs()) - diagonal_steps;
    let capacity = usize::try_from(diagonal_steps * 16 + (axis_steps + 1) * 4)
        .map_err(|_| PlacementFailure::InvalidTopology)?;
    let mut emitted = HashSet::with_capacity(capacity);
    let mut cells = Vec::with_capacity(capacity);
    let mut append = |cell| {
        if emitted.insert(cell) {
            cells.push(cell);
        }
    };
    let width = i64::from(width);
    let triangle_offset = width / 2;
    let step = i64::from(step);
    let mut centre = from;

    // Native line emission walks the shared grid corners for its diagonal
    // portion. Each corner covers the two inward triangles of four squares,
    // rather than placing disconnected whole squares along a Bresenham line.
    for _ in 0..diagonal_steps {
        let corner = offset_cell(centre, delta_x.signum() * width, delta_y.signum() * width)?;
        for (sx, sy) in [(1, 1), (1, -1), (-1, -1), (-1, 1)] {
            let square = offset_cell(corner, sx * width, sy * width)?;
            append(offset_cell(square, -sx * triangle_offset, 0)?);
            append(offset_cell(square, 0, -sy * triangle_offset)?);
            let outer_x = offset_cell(square, sx * triangle_offset, 0)?;
            let outer_y = offset_cell(square, 0, sy * triangle_offset)?;
            // The native pair check preserves an empty outer half and joins
            // it when exactly one of its triangles already contains a path.
            if index.paths.contains_key(&outer_x) != index.paths.contains_key(&outer_y) {
                append(outer_x);
                append(outer_y);
            }
        }
        centre = offset_cell(centre, delta_x.signum() * step, delta_y.signum() * step)?;
    }
    // A purely diagonal run ends at its last corner group. Axis-aligned
    // remainder includes both endpoint squares; a single point emits once.
    if diagonal_steps == 0 || axis_steps != 0 {
        let axis_x = if delta_x.abs() > diagonal_steps {
            delta_x.signum()
        } else {
            0
        };
        let axis_y = if delta_y.abs() > diagonal_steps {
            delta_y.signum()
        } else {
            0
        };
        for position in 0..=axis_steps {
            let square = offset_cell(centre, position * axis_x * step, position * axis_y * step)?;
            for (dx, dy) in [
                (0, triangle_offset),
                (triangle_offset, 0),
                (0, -triangle_offset),
                (-triangle_offset, 0),
            ] {
                append(offset_cell(square, dx, dy)?);
            }
        }
    }
    Ok(cells)
}

#[cfg(test)]
mod ground_path_native_line_tests {
    use super::*;

    #[test]
    fn diagonal_step_emits_inward_triangles_around_shared_corner() {
        let cells = calculate_ground_path_construction_cells(
            IVec3::new(2, 2, 0),
            IVec3::new(6, 6, 0),
            150,
            75,
            &TopologyIndex::default(),
        )
        .expect("native corner group");
        let actual: HashSet<_> = cells.into_iter().collect();
        let expected = [
            (3, 2),
            (2, 3),
            (5, 2),
            (6, 3),
            (3, 6),
            (2, 5),
            (5, 6),
            (6, 5),
        ]
        .map(|(x, y)| IVec3::new(x, y, 0))
        .into_iter()
        .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn diagonal_coverage_is_unchanged_when_endpoints_are_reversed() {
        let index = TopologyIndex::default();
        for end in [IVec3::new(10, 10, 0), IVec3::new(-6, 10, 0)] {
            let start = IVec3::new(2, 2, 0);
            let forward: HashSet<_> =
                calculate_ground_path_construction_cells(start, end, 150, 75, &index)
                    .expect("forward run")
                    .into_iter()
                    .collect();
            let reverse = calculate_ground_path_construction_cells(end, start, 150, 75, &index)
                .expect("reverse run")
                .into_iter()
                .collect();
            assert_eq!(forward, reverse);
            assert_eq!(forward.len(), 16);
        }
    }

    #[test]
    fn axis_run_includes_both_end_squares_and_point_has_one_square() {
        let index = TopologyIndex::default();
        let start = IVec3::new(2, 2, 0);
        assert_eq!(
            calculate_ground_path_construction_cells(start, start, 150, 75, &index)
                .expect("point")
                .len(),
            4
        );
        assert_eq!(
            calculate_ground_path_construction_cells(start, IVec3::new(10, 2, 0), 150, 75, &index)
                .expect("axis run")
                .len(),
            12
        );
    }
}

fn read_fence_construction_facts(
    definitions: WorldDefinitionsView<'_>,
    id: AssetId,
) -> Option<(i64, Option<Gate>, u16)> {
    let fence = definitions.find_fence(id)?;
    let object = definitions.find_object(AssetId(fence.object.0))?;
    let is_gate = matches!(&object.kind, WorldObjectKind::Gate);
    Some((
        object.price_cents,
        is_gate.then_some(Gate {
            open: false,
            locked: false,
        }),
        fence.segment_length_cm,
    ))
}

fn sample_terrain_height_at_topology_cell(
    cell: IVec3,
    grid: TopologyGrid,
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
) -> Option<f32> {
    let world_xz = grid.cell_translation(cell).xz();
    let entity = terrain_chunk_at(terrain_index, world_xz)?;
    let (chunk, edited) = terrain_chunks.get(entity).ok()?;
    let asset = terrain_assets.get(&chunk.asset)?;
    sample_terrain(chunk, asset, edited, world_xz).map(|point| point.height_m)
}

fn path_support_is_eligible(
    definition: &GuestPathDefinition,
    from: IVec3,
    to: IVec3,
    headroom_cm: u32,
) -> bool {
    let horizontal = (to - from).xy().abs().max_element() as u32;
    horizontal != 0
        && (to.z - from.z).unsigned_abs().saturating_mul(1000)
            <= u32::from(definition.max_support_grade_permille).saturating_mul(horizontal)
        && headroom_cm >= u32::from(definition.support_headroom_cm)
}
