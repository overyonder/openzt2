use bevy::prelude::*;
pub(super) mod runtime;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_data::{
    particle::particle_emission_point::ParticleEmissionPoint,
    terrain::{TerrainWaterBubbleDetail, TerrainWaterSuspensionEmission},
};
use openzt2_world_state::simulation_time::native_visual_random_stream::NativeVisualRandomStream;
use openzt2_zoo_state::terrain::{
    terrain_water_presentation_types::TerrainWaterMembershipIndex,
    terrain_water_underwater_particle_types::TerrainWaterSuspensionVolume,
};

pub(super) fn select_bubble_detail(
    details: &[TerrainWaterBubbleDetail],
    quality: u32,
) -> Option<&TerrainWaterBubbleDetail> {
    let mut selected = None;
    for detail in details {
        if detail.level > quality {
            break;
        }
        selected = Some(detail);
    }
    selected
}

pub(super) fn suspension_emitter_density(
    emission: &TerrainWaterSuspensionEmission,
    array_index: usize,
    depth_ratio: f32,
) -> f32 {
    let select = |values: &[f32], fallback| {
        values
            .get(array_index)
            .or_else(|| values.last())
            .copied()
            .unwrap_or(fallback)
    };
    let minimum = select(&emission.minimum_density, 0.0);
    let maximum = select(&emission.maximum_density, 0.5);
    (minimum + (maximum - minimum) * depth_ratio).clamp(0.0, 1.0)
}

pub(super) fn bubble_spawn_delay_seconds(
    small_seconds: f32,
    big_seconds: f32,
    render_area_m2: f32,
    random: &mut NativeVisualRandomStream,
) -> f32 {
    let area_fraction = ((f64::from(render_area_m2) - 100.0) / 3500.0).clamp(0.0, 1.0) as f32;
    let interpolated_seconds = small_seconds + (big_seconds - small_seconds) * area_fraction;
    (f64::from(interpolated_seconds) * (0.5 + 1.5 * random.unit_f64())) as f32
}

pub(super) fn bubble_spawn_point(
    owner: Entity,
    membership: &TerrainWaterMembershipIndex,
    terrain: &TerrainAsset,
    surface_height: f32,
    random: &mut NativeVisualRandomStream,
) -> Option<Vec3> {
    let count = membership.accepted_points(owner).count();
    if count == 0 {
        return None;
    }
    let index = (random.unit_f64() * (count as f64 - f64::from(0.01_f32))).trunc() as usize;
    let point = membership.accepted_points(owner).nth(index)?;
    for offset in [
        IVec2::new(-3, -3),
        IVec2::new(3, -3),
        IVec2::new(3, 3),
        IVec2::new(-3, 3),
    ] {
        let corner = point.as_ivec2() + offset;
        if corner.min_element() < 0 || membership.point_owner(corner.as_uvec2()) != Some(owner) {
            return None;
        }
    }
    let spacing = terrain.canonical_terrain_grid().units.cell_size_metres;
    Some(Vec3::new(
        point.x as f32 * spacing,
        surface_height,
        -(point.y as f32) * spacing,
    ))
}

pub(super) fn append_suspension_points(
    points: &mut Vec<ParticleEmissionPoint>,
    volume: TerrainWaterSuspensionVolume,
    particle_radius: f32,
    count: u32,
    random: &mut NativeVisualRandomStream,
) {
    for _ in 0..count {
        let x = (random.unit_f64() * f64::from(volume.size.x)) as f32;
        let source_y = (random.unit_f64() * f64::from(volume.size.z)) as f32;
        let y = (f64::from(volume.size.y - 2.0 * particle_radius) * random.unit_f64()) as f32;
        if y >= 0.0 {
            points.push(ParticleEmissionPoint {
                position: (volume.origin + Vec3::new(x, y + particle_radius, -source_y)).to_array(),
                velocity: [0.0; 3],
            });
        }
    }
}

pub(super) fn append_initial_suspension_emission(
    points: &mut Vec<ParticleEmissionPoint>,
    volumes: &[TerrainWaterSuspensionVolume],
    density: f32,
    particle_radius: f32,
    random: &mut NativeVisualRandomStream,
) {
    let total_volume: f32 = volumes.iter().map(|volume| volume.volume_m3()).sum();
    let target = (density * total_volume).min(10_000.0);
    if total_volume <= 0.0 || target < 1.0 {
        return;
    }
    for &volume in volumes {
        let expected = volume.volume_m3() * target / total_volume;
        let integral = expected.trunc() as u32;
        let count = integral + u32::from(random.unit_f64() < f64::from(expected - integral as f32));
        append_suspension_points(points, volume, particle_radius, count, random);
    }
}

pub(super) fn append_suspension_density_top_up(
    points: &mut Vec<ParticleEmissionPoint>,
    volumes: &[TerrainWaterSuspensionVolume],
    density: f32,
    particle_radius: f32,
    live_count: u32,
    random: &mut NativeVisualRandomStream,
) {
    let total_volume: f32 = volumes.iter().map(|volume| volume.volume_m3()).sum();
    if total_volume <= 0.0 || volumes.is_empty() {
        return;
    }
    let target = (density * total_volume).min(10_000.0);
    let mut gap = target - live_count as f32;
    while gap > 1.0 {
        let selected =
            (random.unit_f64() * volumes.len() as f64 - f64::from(0.01_f32)).trunc() as usize;
        let volume = volumes[selected];
        let count = (f64::from(volume.volume_m3() * target / total_volume) + 1.0).trunc() as u32;
        append_suspension_points(points, volume, particle_radius, count, random);
        gap -= count as f32;
    }
}
