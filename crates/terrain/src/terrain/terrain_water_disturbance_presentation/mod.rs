use bevy::prelude::*;
use openzt2_game_assets::effect::SpawnCompleteParticleEffectDocument;
use openzt2_game_assets::graphics_settings_types::{FourLevelGraphicsDetail, GraphicsSettings};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_data::terrain::{TerrainWaterConfiguration, TerrainWaterSprayVariant};
use openzt2_game_data::AssetId;
use openzt2_world_state::simulation_time::native_visual_random_stream::NativeVisualRandomStream;
use openzt2_zoo_state::audio::audio_playback_message_types::PlayPositionedAudioCue;
use openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::WaterSurfaceSpraySample;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionQuery;

pub(super) mod wake_mesh_emission;

#[allow(clippy::too_many_arguments)]
pub(super) fn present_authored_water_surface_spray_samples(
    mut samples: MessageReader<WaterSurfaceSpraySample>,
    surfaces: TerrainWaterRegionQuery,
    terrain_assets: Res<Assets<TerrainAsset>>,
    graphics: Res<GraphicsSettings>,
    mut random: ResMut<NativeVisualRandomStream>,
    mut particles: MessageWriter<SpawnCompleteParticleEffectDocument>,
    mut sounds: MessageWriter<PlayPositionedAudioCue>,
    mut selection: Local<u64>,
) {
    let level = match graphics.water_detail {
        FourLevelGraphicsDetail::Low => 0,
        FourLevelGraphicsDetail::Medium => 1,
        FourLevelGraphicsDetail::High => 2,
        FourLevelGraphicsDetail::Ultra => 3,
    };
    for sample in samples.read() {
        let Some(surface) = surfaces.get(sample.surface) else {
            continue;
        };
        let Some(terrain) = terrain_assets.get(surface.terrain()) else {
            continue;
        };
        let Some(presentation) = surface
            .biome(terrain)
            .and_then(|biome| biome.water_presentation.as_ref())
        else {
            continue;
        };
        let Some(variant) = select_authored_spray_variant(
            &presentation.surface_material.configuration,
            level,
            sample.native_strength,
        ) else {
            continue;
        };
        if variant.system.is_empty() {
            continue;
        }
        let Some(height) = surface.surface_height(terrain) else {
            continue;
        };
        let position = Vec3::new(sample.source_position.x, height, -sample.source_position.y);
        for (cue, above, below) in [
            (&variant.above_water_sound, true, false),
            (&variant.below_water_sound, false, true),
        ] {
            if let Some(cue) = cue.as_deref().filter(|cue| !cue.is_empty()) {
                *selection = selection.wrapping_add(1);
                sounds.write(PlayPositionedAudioCue {
                    cue: AssetId::from_key(&format!(
                        "audio-cue:{}",
                        cue.trim().to_ascii_lowercase()
                    )),
                    world_position: position,
                    selection: *selection,
                    priority: 0,
                    listener_above_water: above,
                    listener_below_water: below,
                    admission: None,
                });
            }
        }
        let Some(effect) = terrain.water_particle_effect(&variant.system) else {
            continue;
        };
        particles.write(SpawnCompleteParticleEffectDocument {
            particle_effect_asset: effect.clone(),
            parent_entity: None,
            effect_transform: Transform {
                translation: position,
                rotation: authored_splash_rotation(sample.direction, sample.normal, &mut random),
                ..default()
            },
            manual_particle_count: 0,
        });
    }
}

fn select_authored_spray_variant(
    configuration: &TerrainWaterConfiguration,
    level: u32,
    strength: f32,
) -> Option<&TerrainWaterSprayVariant> {
    let variants = configuration
        .spray
        .iter()
        .take_while(|branch| branch.level <= level)
        .last()
        .map(|branch| branch.variants.as_slice())
        .unwrap_or(&configuration.spray_fallback);
    variants
        .iter()
        .find(|variant| strength <= variant.maximum_strength)
        .or_else(|| variants.last())
}

fn authored_splash_rotation(
    direction: Vec3,
    normal: Vec3,
    random: &mut NativeVisualRandomStream,
) -> Quat {
    let normal = normal.normalize_or_zero();
    let right = direction.cross(normal);
    let right = if right.length() < 0.0001 {
        let mut tangent = Vec3::X.cross(normal);
        if tangent.length_squared() < 0.01 {
            tangent = Vec3::NEG_Z.cross(normal);
        }
        let tangent = tangent.normalize_or_zero();
        let second = normal.cross(tangent);
        let half_turn = f64::from(std::f32::consts::PI);
        let angle = (-half_turn + (half_turn + half_turn) * random.unit_f64()) as f32;
        let (sine, cosine) = angle.sin_cos();
        tangent * cosine + second * sine
    } else {
        right.normalize()
    };
    Quat::from_mat3(&Mat3::from_cols(right, normal, right.cross(normal)))
}
