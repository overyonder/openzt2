use bevy::prelude::{Mesh, Vec2};
use openzt2_zoo_state::terrain::terrain_water_shoreline_geometry_types::TerrainShorelineProfile;
use openzt2_zoo_state::terrain::terrain_water_shoreline_geometry_types::TerrainShorelineProfileError;
use openzt2_zoo_state::terrain::terrain_water_shoreline_geometry_types::TerrainShorelineProfilePoint;

mod boundary_tracing;
mod mesh_construction;
mod patch_deformation;
pub(crate) mod projection;
mod source_geometry;
pub(crate) mod stitch_geometry;

pub(crate) struct ConstructedTerrainShorelineMeshGroup {
    pub(crate) mesh: Mesh,
    pub(crate) biome_index: u8,
    pub(crate) surface_plane_height: f32,
}

/// Folds accumulated longitudinal coordinates around the first sample beyond
/// half the profile length, with an integral number of texture repetitions.
///
/// # Errors
/// Rejects empty or zero-length profiles, invalid point coordinates and a
/// nonpositive or nonfinite distance scale.
pub(crate) fn construct_terrain_shoreline_profile(
    mut points: Box<[TerrainShorelineProfilePoint]>,
    distance_scale: f32,
) -> Result<TerrainShorelineProfile, TerrainShorelineProfileError> {
    if points.is_empty() {
        return Err(TerrainShorelineProfileError::Empty);
    }
    if points.iter().any(|point| {
        !point.position.is_finite()
            || !point.longitudinal_coordinate.is_finite()
            || point.longitudinal_coordinate < 0.0
    }) {
        return Err(TerrainShorelineProfileError::InvalidPoint);
    }
    let maximum = points
        .iter()
        .map(|point| point.longitudinal_coordinate)
        .fold(0.0_f32, f32::max);
    if maximum == 0.0 {
        return Err(TerrainShorelineProfileError::ZeroLength);
    }
    let half = maximum * 0.5;
    let pivot = points
        .iter()
        .map(|point| point.longitudinal_coordinate)
        .filter(|coordinate| *coordinate > half)
        .fold(maximum, f32::min);
    let repeats = half.ceil();
    for point in &mut points {
        let coordinate = point.longitudinal_coordinate;
        point.longitudinal_coordinate = if coordinate <= pivot {
            (coordinate / pivot) * repeats
        } else {
            ((maximum - coordinate) / (maximum - pivot)) * repeats
        };
    }
    TerrainShorelineProfile::from_normalized_points(points, distance_scale)
}

/// Samples the closed boundary. The second coordinate is negative above water
/// and positive at or below the water plane.
pub(crate) fn sample_terrain_shoreline_profile(
    profile: &TerrainShorelineProfile,
    position: Vec2,
    terrain_height: f32,
    water_height: f32,
) -> Vec2 {
    let points = profile.points();
    let mut nearest_distance_squared = f32::INFINITY;
    let mut longitudinal_coordinate = 0.0;
    for (start, end) in points.iter().zip(points.iter().cycle().skip(1)) {
        let direction = end.position - start.position;
        let projection = (position - start.position).dot(direction);
        let parameter = if projection <= 0.0 {
            0.0
        } else {
            let length_squared = direction.length_squared();
            if projection >= length_squared {
                1.0
            } else {
                projection / length_squared
            }
        };
        let closest = start.position + direction * parameter;
        let distance_squared = closest.distance_squared(position);
        if distance_squared < nearest_distance_squared {
            nearest_distance_squared = distance_squared;
            longitudinal_coordinate = start.longitudinal_coordinate
                + (end.longitudinal_coordinate - start.longitudinal_coordinate) * parameter;
        }
    }
    let distance = nearest_distance_squared.sqrt() / profile.distance_scale();
    Vec2::new(
        longitudinal_coordinate,
        if terrain_height > water_height {
            -distance
        } else {
            distance
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_profile_samples_last_to_first_segment_and_signed_distance(
    ) -> Result<(), TerrainShorelineProfileError> {
        let profile = construct_terrain_shoreline_profile(
            [
                (Vec2::ZERO, 0.0),
                (Vec2::new(2.0, 0.0), 2.0),
                (Vec2::new(2.0, 2.0), 4.0),
                (Vec2::new(0.0, 2.0), 6.0),
            ]
            .map(
                |(position, longitudinal_coordinate)| TerrainShorelineProfilePoint {
                    position,
                    longitudinal_coordinate,
                },
            )
            .into(),
            2.0,
        )?;
        assert_eq!(
            sample_terrain_shoreline_profile(&profile, Vec2::new(-1.0, 1.0), 1.0, 0.0),
            Vec2::new(0.0, -0.5),
        );
        assert_eq!(
            sample_terrain_shoreline_profile(&profile, Vec2::new(3.0, 1.0), 0.0, 0.0),
            Vec2::new(2.25, 0.5),
        );
        Ok(())
    }
}
