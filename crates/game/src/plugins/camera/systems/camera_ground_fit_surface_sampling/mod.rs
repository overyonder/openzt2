use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_chunk_types::TerrainIndex;
use crate::plugins::terrain::terrain_world_sampling::sample_terrain;
use crate::plugins::terrain::terrain_world_sampling::terrain_chunk_at;

pub(super) fn sample_highest_terrain_or_water_fitting_surface_height_in_three_by_three_metre_neighborhood(
    world_horizontal_position: Vec2,
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
) -> Option<f32> {
    [-1.0_f32, 0.0, 1.0]
        .into_iter()
        .flat_map(|z| {
            [-1.0_f32, 0.0, 1.0]
                .into_iter()
                .map(move |x| world_horizontal_position + Vec2::new(x, z))
        })
        .filter_map(|point| {
            terrain_chunk_at(terrain_index, point)
                .and_then(|entity| terrain_chunks.get(entity).ok())
                .and_then(|(chunk, edited)| {
                    let asset = terrain_assets.get(&chunk.asset)?;
                    sample_terrain(chunk, asset, edited, point)
                })
                .map(|terrain| {
                    terrain
                        .water_height_m
                        .unwrap_or(terrain.height_m)
                        .max(terrain.height_m)
                })
        })
        .reduce(f32::max)
}

/// Samples terrain and prefab collisions through Avian, then includes the
/// horizontal water surface from the terrain sample.
pub(super) fn sample_highest_active_collision_terrain_or_water_fitting_surface_height_in_three_by_three_metre_neighborhood(
    world_horizontal_position: Vec2,
    spatial_query: &SpatialQuery,
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
) -> Option<f32> {
    const FITTING_PROBE_TOP_METRES: f32 = 10_000.0;
    const FITTING_PROBE_LENGTH_METRES: f32 = 20_000.0;

    let collision_height = [-1.0_f32, 0.0, 1.0]
        .into_iter()
        .flat_map(|z| {
            [-1.0_f32, 0.0, 1.0]
                .into_iter()
                .map(move |x| world_horizontal_position + Vec2::new(x, z))
        })
        .filter_map(|point| {
            let origin = Vec3::new(point.x, FITTING_PROBE_TOP_METRES, point.y);
            spatial_query
                .cast_ray(
                    origin,
                    Dir3::NEG_Y,
                    FITTING_PROBE_LENGTH_METRES,
                    // A probe outside the map is inside a vertical boundary
                    // half-space. Its interior is not a surface at probe height.
                    false,
                    &SpatialQueryFilter::DEFAULT,
                )
                .map(|hit| {
                    let height = origin.y - hit.distance;
                    trace!(target: "openzt2_camera_fitting", ?point, entity = ?hit.entity,
                        height, distance = hit.distance, "camera fitting collision hit");
                    height
                })
        })
        .reduce(f32::max);
    let terrain_or_water_height =
        sample_highest_terrain_or_water_fitting_surface_height_in_three_by_three_metre_neighborhood(
            world_horizontal_position,
            terrain_index,
            terrain_assets,
            terrain_chunks,
        );
    collision_height
        .into_iter()
        .chain(terrain_or_water_height)
        .reduce(f32::max)
}

#[cfg(test)]
mod tests {
    use avian3d::prelude::Collider;
    use bevy::prelude::*;

    #[test]
    fn camera_surface_probe_ignores_vertical_boundary_interiors_but_hits_ground() {
        for normal in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
            let boundary = Collider::half_space(normal);
            let origin = Vec3::Y * 10_000.0 - normal;
            assert!(boundary
                .cast_ray(
                    Vec3::ZERO,
                    Quat::IDENTITY,
                    origin,
                    Vec3::NEG_Y,
                    20_000.0,
                    false
                )
                .is_none());
        }
        let ground = Collider::cuboid(20.0, 2.0, 20.0);
        let hit = ground.cast_ray(
            Vec3::Y * 9.0,
            Quat::IDENTITY,
            Vec3::Y * 10_000.0,
            Vec3::NEG_Y,
            20_000.0,
            false,
        );
        assert!(
            hit.is_some_and(|(distance, normal)| (distance - 9990.0).abs() < 0.01
                && normal.abs_diff_eq(Vec3::Y, 0.0001))
        );
    }
}
