//! Terrain caustic projection and texture animation.

#[cfg(test)]
mod animation_tests;
mod ground_mesh;
mod pass_projection;
pub(super) mod scene_projection;
pub(super) mod systems;

use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_data::terrain::TerrainWaterCausticsConfiguration;
use openzt2_zoo_state::terrain::terrain_water_caustic_types::{
    TerrainWaterCausticAnimation, TerrainWaterCausticRock,
};

pub(super) fn source_from_bevy_world() -> Mat4 {
    let basis = [Vec3::X, Vec3::Y, Vec3::Z].map(|axis| {
        Vec3::from_array(convert_source_z_up_vector_to_bevy_y_up_coordinates(
            axis.to_array(),
        ))
    });
    Mat4::from_mat3(Mat3::from_cols(basis[0], basis[1], basis[2]).transpose())
}

pub(super) fn bind_caustic_textures(
    pass: &mut EffectPassMaterial,
    asset: &TerrainAsset,
    configuration: &TerrainWaterCausticsConfiguration,
) -> bool {
    let (Some(bottom), Some(top), Some(height)) = (
        asset.texture_image(&configuration.bottom_map),
        asset.texture_image(&configuration.top_map),
        asset.texture_image(&configuration.height_lookup),
    ) else {
        return false;
    };
    let bottom_bound =
        pass.replace_texture_asset_for_effect_semantic("CausticMap0", bottom.clone());
    let top_bound = pass.replace_texture_asset_for_effect_semantic("CausticMap1", top.clone());
    let height_bound = pass.replace_texture_asset_for_effect_semantic("HeightLUT", height.clone());
    bottom_bound
        && top_bound
        && (height_bound || pass.has_vertex_float_effect_semantic("HeightRamp"))
}

pub(super) fn bind_caustic_camera_projection(
    pass: &mut EffectPassMaterial,
    world_projection: [Mat4; 3],
    camera_to_source_world: Mat4,
    buffers: &mut Assets<ShaderBuffer>,
) -> std::io::Result<bool> {
    let mut all_bound = true;
    for (semantic, matrix) in ["CameraToCaustic0", "CameraToCaustic1", "CameraToHeight"]
        .into_iter()
        .zip(world_projection)
    {
        all_bound &= pass.bind_fixed_function_matrix_effect_semantic(
            semantic,
            matrix * camera_to_source_world,
        )?;
    }
    if all_bound {
        pass.write_changed_fixed_function_transforms_to_persistent_buffer(buffers);
    }
    Ok(all_bound)
}

/// The caller supplies the shared random stream as unit-interval samples.
pub(super) fn initialize_caustic_animation(
    configuration: &TerrainWaterCausticsConfiguration,
    random_unit: &mut impl FnMut() -> f64,
) -> TerrainWaterCausticAnimation {
    TerrainWaterCausticAnimation {
        bottom_phase: Vec2::ZERO,
        top_phase: Vec2::ZERO,
        bottom_rock: initialize_rock(
            configuration.bottom_rock_rate_range,
            configuration.bottom_rock_distance_range,
            random_unit,
        ),
        top_rock: initialize_rock(
            configuration.top_rock_rate_range,
            configuration.top_rock_distance_range,
            random_unit,
        ),
    }
}

pub(super) fn advance_caustic_animation(
    state: &mut TerrainWaterCausticAnimation,
    configuration: &TerrainWaterCausticsConfiguration,
    delta_seconds: f32,
    random_unit: &mut impl FnMut() -> f64,
) {
    advance_rock(
        &mut state.bottom_rock,
        configuration.bottom_rock_rate_range,
        configuration.bottom_rock_distance_range,
        delta_seconds,
        random_unit,
    );
    advance_rock(
        &mut state.top_rock,
        configuration.top_rock_rate_range,
        configuration.top_rock_distance_range,
        delta_seconds,
        random_unit,
    );
    state.bottom_phase = (state.bottom_phase
        + Vec2::from_array(configuration.bottom_rate) * delta_seconds)
        .rem_euclid(Vec2::ONE);
    state.top_phase = (state.top_phase + Vec2::from_array(configuration.top_rate) * delta_seconds)
        .rem_euclid(Vec2::ONE);
}

/// Matrices consume source coordinates: horizontal x/y and vertical z.
pub(super) fn caustic_world_projection_matrices(
    state: &TerrainWaterCausticAnimation,
    configuration: &TerrainWaterCausticsConfiguration,
    surface_height: f32,
) -> [Mat4; 3] {
    let bottom_offset =
        (state.bottom_phase + Vec2::splat(rock_offset(state.bottom_rock))).rem_euclid(Vec2::ONE);
    let top_offset =
        (state.top_phase + Vec2::splat(rock_offset(state.top_rock))).rem_euclid(Vec2::ONE);
    let fade_start_height = surface_height - configuration.fade_start;
    let fade_end_height = fade_start_height - configuration.fade_distance;
    let height_scale = (fade_end_height - fade_start_height).recip();
    let height_offset = -fade_start_height * height_scale;
    [
        Mat4::from_cols(
            Vec4::new(configuration.bottom_tile, 0.0, 0.0, 0.0),
            Vec4::new(0.0, configuration.bottom_tile, 0.0, 0.0),
            Vec4::Z,
            bottom_offset.extend(0.0).extend(1.0),
        ),
        Mat4::from_cols(
            Vec4::new(0.0, configuration.top_tile, 0.0, 0.0),
            Vec4::new(configuration.top_tile, 0.0, 0.0, 0.0),
            Vec4::Z,
            top_offset.extend(0.0).extend(1.0),
        ),
        Mat4::from_cols(
            Vec4::Y,
            Vec4::Z,
            Vec4::new(height_scale, 0.0, 0.0, 0.0),
            Vec4::new(height_offset, 0.0, 0.0, 1.0),
        ),
    ]
}

fn initialize_rock(
    rate_range: [f32; 2],
    distance_range: [f32; 2],
    random_unit: &mut impl FnMut() -> f64,
) -> TerrainWaterCausticRock {
    TerrainWaterCausticRock {
        phase: random_unit() as f32,
        rate: sample_range(rate_range, random_unit()),
        distance: sample_range(distance_range, random_unit()),
    }
}

fn advance_rock(
    rock: &mut TerrainWaterCausticRock,
    rate_range: [f32; 2],
    distance_range: [f32; 2],
    delta_seconds: f32,
    random_unit: &mut impl FnMut() -> f64,
) {
    rock.phase += rock.rate * delta_seconds;
    if rock.phase > 1.0 {
        rock.phase = 1.0;
        rock.rate = -rock.rate;
    } else if rock.phase < 0.0 {
        rock.phase = 0.0;
        rock.rate = sample_range(rate_range, random_unit());
        rock.distance = sample_range(distance_range, random_unit());
    }
}

fn sample_range([minimum, maximum]: [f32; 2], unit: f64) -> f32 {
    ((f64::from(maximum) - f64::from(minimum)) * unit + f64::from(minimum)) as f32
}

fn rock_offset(rock: TerrainWaterCausticRock) -> f32 {
    let phase = f64::from(rock.phase);
    let eased = (3.0 * phase * phase - 2.0 * phase * phase * phase) as f32;
    eased.clamp(0.0, 1.0) * rock.distance
}
