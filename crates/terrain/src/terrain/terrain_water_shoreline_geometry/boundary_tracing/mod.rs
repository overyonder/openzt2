use std::collections::BTreeSet;

use bevy::prelude::IVec2;
use openzt2_zoo_state::terrain::terrain_water_shoreline_geometry_types::{
    TerrainShorelineBoundary, TerrainShorelineBoundaryEdge,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ShorelineTraceError {
    InvalidSeed,
    RepeatedEdge,
}

/// The candidate predicate queries global terrain point membership so a chunk
/// edge never acts as a boundary in its own right.
pub(super) fn trace_shoreline_boundary(
    seed: TerrainShorelineBoundaryEdge,
    mut is_candidate: impl FnMut(TerrainShorelineBoundaryEdge) -> bool,
) -> Result<TerrainShorelineBoundary, ShorelineTraceError> {
    let direction = seed.water - seed.land;
    if direction.abs().element_sum() != 1 || !is_candidate(seed) {
        return Err(ShorelineTraceError::InvalidSeed);
    }
    let (clockwise, closed) = walk(seed, true, &mut is_candidate)?;
    if closed {
        return Ok(TerrainShorelineBoundary {
            edges: clockwise.into_boxed_slice(),
            closed: true,
        });
    }
    let end = clockwise[clockwise.len() - 1];
    let (counterclockwise, _) = walk(end, false, &mut is_candidate)?;
    let start = counterclockwise[counterclockwise.len() - 1];
    let (edges, closed) = walk(start, true, &mut is_candidate)?;
    Ok(TerrainShorelineBoundary {
        edges: edges.into_boxed_slice(),
        closed,
    })
}

fn walk(
    seed: TerrainShorelineBoundaryEdge,
    clockwise: bool,
    is_candidate: &mut impl FnMut(TerrainShorelineBoundaryEdge) -> bool,
) -> Result<(Vec<TerrainShorelineBoundaryEdge>, bool), ShorelineTraceError> {
    let mut edges = vec![seed];
    let mut seen = BTreeSet::from([edge_key(seed)]);
    let mut current = seed;
    loop {
        let direction = current.water - current.land;
        let tangent = if clockwise {
            IVec2::new(direction.y, -direction.x)
        } else {
            IVec2::new(-direction.y, direction.x)
        };
        let candidates = [
            TerrainShorelineBoundaryEdge {
                land: current.land,
                water: current.land + tangent,
            },
            TerrainShorelineBoundaryEdge {
                land: current.land + tangent,
                water: current.water + tangent,
            },
            TerrainShorelineBoundaryEdge {
                land: current.water + tangent,
                water: current.water,
            },
        ];
        let Some(next) = candidates.into_iter().find(|edge| is_candidate(*edge)) else {
            return Ok((edges, false));
        };
        if next == seed {
            return Ok((edges, true));
        }
        if !seen.insert(edge_key(next)) {
            return Err(ShorelineTraceError::RepeatedEdge);
        }
        edges.push(next);
        current = next;
    }
}

fn edge_key(edge: TerrainShorelineBoundaryEdge) -> (i32, i32, i32, i32) {
    (edge.land.x, edge.land.y, edge.water.x, edge.water.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolated_land_point_traces_all_four_edges_without_a_chunk_seam(
    ) -> Result<(), ShorelineTraceError> {
        let boundary = trace_shoreline_boundary(
            TerrainShorelineBoundaryEdge {
                land: IVec2::ZERO,
                water: IVec2::X,
            },
            |edge| edge.land == IVec2::ZERO && edge.water.abs().element_sum() == 1,
        )?;
        assert!(boundary.closed);
        assert_eq!(
            boundary
                .edges
                .iter()
                .map(|edge| edge.water)
                .collect::<Vec<_>>(),
            vec![IVec2::X, IVec2::NEG_Y, IVec2::NEG_X, IVec2::Y],
        );
        Ok(())
    }
}
