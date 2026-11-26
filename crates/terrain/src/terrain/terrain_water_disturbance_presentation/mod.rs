use bevy::prelude::*;
use openzt2_game_assets::effect::SpawnCompleteParticleEffectDocument;
use openzt2_game_assets::graphics_settings_types::{FourLevelGraphicsDetail, GraphicsSettings};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_data::terrain::{TerrainWaterConfiguration, TerrainWaterSprayVariant};
use openzt2_game_data::AssetId;
use openzt2_world::physics::water_surface_disturbance_projection::NamedWaterSurfaceEffect;
use openzt2_world_state::simulation_time::native_visual_random_stream::NativeVisualRandomStream;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::audio::audio_playback_message_types::PlayPositionedAudioCue;
use openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::WaterSurfaceSpraySample;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionQuery,
};

pub(super) mod wake_mesh_emission;

#[derive(Component, Default)]
pub(super) struct TerrainWaterSprayAudioSequence(u64);

pub(super) fn present_named_water_surface_effects(
    mut samples: MessageReader<NamedWaterSurfaceEffect>,
    surfaces: TerrainWaterRegionQuery,
    memberships: Query<&TerrainWaterMembershipIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    server: Res<AssetServer>,
    mut particles: MessageWriter<SpawnCompleteParticleEffectDocument>,
) {
    for sample in samples.read() {
        let Some(surface) = surfaces.get(sample.surface) else {
            continue;
        };
        let Some(terrain) = terrain_assets.get(surface.terrain()) else {
            continue;
        };
        let Some(height) = surface.surface_height(terrain) else {
            continue;
        };
        let spacing = terrain.canonical_terrain_grid().units.cell_size_metres;
        let point = sample.source_position / spacing;
        if !point.is_finite() || point.min_element() < 0.0 {
            continue;
        }
        let tile = point.as_uvec2() / 4;
        if !memberships.iter().any(|index| {
            index.terrain() == surface.terrain().id()
                && index
                    .side_owners(tile)
                    .is_some_and(|slots| slots.contains(&Some(sample.surface)))
        }) {
            continue;
        }
        let Some(effect) = terrain.load_named_water_particle_effect(sample.variant, &server) else {
            continue;
        };
        let mut transform = sample.joint_pose.compute_transform();
        transform.translation =
            Vec3::new(sample.source_position.x, height, -sample.source_position.y);
        particles.write(SpawnCompleteParticleEffectDocument {
            particle_effect_asset: effect,
            parent_entity: None,
            inherit_parent_rotation: true,
            effect_transform: transform,
            manual_particle_count: 0,
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn present_authored_water_surface_spray_samples(
    mut samples: MessageReader<WaterSurfaceSpraySample>,
    surfaces: TerrainWaterRegionQuery,
    terrain_assets: Res<Assets<TerrainAsset>>,
    graphics: Res<GraphicsSettings>,
    mut random: ResMut<NativeVisualRandomStream>,
    mut particles: MessageWriter<SpawnCompleteParticleEffectDocument>,
    mut sounds: MessageWriter<PlayPositionedAudioCue>,
    members: Query<&WorldMember>,
    parents: Query<&ChildOf>,
    mut sequences: Query<&mut TerrainWaterSprayAudioSequence>,
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
        if let Some(mut sequence) = std::iter::once(sample.surface)
            .chain(parents.iter_ancestors::<ChildOf>(sample.surface))
            .find_map(|entity| members.get(entity).ok())
            .and_then(|member| sequences.get_mut(member.root).ok())
        {
            for (cue, above, below) in [
                (&variant.above_water_sound, true, false),
                (&variant.below_water_sound, false, true),
            ] {
                if let Some(cue) = cue.as_deref().filter(|cue| !cue.is_empty()) {
                    sequence.0 = sequence.0.wrapping_add(1);
                    sounds.write(PlayPositionedAudioCue {
                        cue: AssetId::from_key(&format!(
                            "audio-cue:{}",
                            cue.trim().to_ascii_lowercase()
                        )),
                        world_position: position,
                        selection: sequence.0,
                        priority: 0,
                        force_looped: false,
                        listener_above_water: above,
                        listener_below_water: below,
                        admission: None,
                    });
                }
            }
        }
        let Some(effect) = terrain.water_particle_effect(&variant.system) else {
            continue;
        };
        particles.write(SpawnCompleteParticleEffectDocument {
            particle_effect_asset: effect.clone(),
            parent_entity: None,
            inherit_parent_rotation: true,
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
