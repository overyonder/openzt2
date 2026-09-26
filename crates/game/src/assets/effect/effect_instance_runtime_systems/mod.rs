//! Dependency binding, emission timing, clocks, stopping, and retirement for live effects.

use bevy::prelude::*;
use bevy_hanabi::prelude::{EffectMaterial, EffectMesh};
use openzt2_game_data::particle::{
    authored_particle_system::{
        AuthoredParticleSystemModifier as EffectModifier,
        ParticleEffectSimulationClock as EffectClock,
    },
    ParticleEffectDocument,
};

use super::{
    effect_runtime_types::{
        ControlledEffectEmissionState, FiniteEffectInstanceLifetime, PendingEffectMaterialBinding,
        PendingEffectMeshBinding,
    },
    LiveParticleEffectInstance, ParticleEffectDocumentAsset, StopParticleEffectsAttachedToEntity,
};
use crate::assets::material::material_asset_types::MaterialAsset;

pub(super) fn stop_effect_instances_attached_to_requested_entities(
    mut commands: Commands,
    mut stop_requests: MessageReader<StopParticleEffectsAttachedToEntity>,
    effect_instances: Query<(Entity, Option<&ChildOf>), With<LiveParticleEffectInstance>>,
) {
    for stop_request in stop_requests.read() {
        effect_instances
            .iter()
            .filter(|(_, parent)| {
                parent.is_some_and(|parent| parent.parent() == stop_request.attached_entity)
            })
            .for_each(|(effect_entity, _)| commands.entity(effect_entity).despawn());
    }
}

pub(super) fn bind_loaded_material_textures_to_pending_effect_instances(
    mut commands: Commands,
    pending_material_bindings: Query<(Entity, &PendingEffectMaterialBinding)>,
    material_assets: Res<Assets<MaterialAsset>>,
) {
    for (effect_entity, pending_material_binding) in &pending_material_bindings {
        let Some(effect_image) = material_assets
            .get(&pending_material_binding.0)
            .and_then(MaterialAsset::first_bound_texture_asset)
            .cloned()
        else {
            continue;
        };
        commands
            .entity(effect_entity)
            .remove::<PendingEffectMaterialBinding>()
            .insert(EffectMaterial {
                images: vec![effect_image],
            });
    }
}

pub(super) fn bind_loaded_named_meshes_to_pending_effect_instances(
    mut commands: Commands,
    pending_mesh_bindings: Query<(Entity, &PendingEffectMeshBinding)>,
    gltf_assets: Res<Assets<bevy::gltf::Gltf>>,
    gltf_mesh_assets: Res<Assets<bevy::gltf::GltfMesh>>,
) {
    for (effect_entity, pending_mesh_binding) in &pending_mesh_bindings {
        let Some(effect_mesh) = gltf_assets
            .get(&pending_mesh_binding.model_asset)
            .and_then(|gltf_asset| {
                gltf_asset
                    .named_meshes
                    .get(pending_mesh_binding.named_mesh_asset_path.as_str())
            })
            .and_then(|gltf_mesh_handle| gltf_mesh_assets.get(gltf_mesh_handle))
            .and_then(|gltf_mesh| gltf_mesh.primitives.first())
            .map(|gltf_primitive| gltf_primitive.mesh.clone())
        else {
            continue;
        };
        commands
            .entity(effect_entity)
            .remove::<PendingEffectMeshBinding>()
            .insert(EffectMesh(effect_mesh));
    }
}

pub(super) fn update_controlled_effect_emission_from_authored_timing(
    presentation_time: Res<Time<Real>>,
    zoo_simulation_time: Res<Time<Virtual>>,
    effect_assets: Res<Assets<ParticleEffectDocumentAsset>>,
    mut effect_instances: Query<(
        &LiveParticleEffectInstance,
        &mut ControlledEffectEmissionState,
        &mut bevy_hanabi::prelude::EffectSpawner,
    )>,
) {
    for (effect_instance, mut emission_state, mut effect_spawner) in &mut effect_instances {
        let Some(source) = effect_assets.get(&effect_instance.effect_asset) else {
            effect_spawner.active = false;
            effect_spawner.spawn_count = 0;
            continue;
        };
        let authored_emitter = match &source.particle_document {
            ParticleEffectDocument::Effect(document) => document
                .emitters
                .iter()
                .find(|emitter| emitter.emitter_id == effect_instance.emitter_id),
            ParticleEffectDocument::LegacyNif(_) => None,
        };
        let authored_emission_window = authored_emitter.and_then(|emitter| {
            emitter
                .modifiers
                .iter()
                .find_map(|modifier| match modifier {
                    EffectModifier::EmitWindow {
                        start_seconds,
                        stop_seconds,
                        frequency,
                        phase,
                    } => Some((*start_seconds, *stop_seconds, *frequency, *phase)),
                    _ => None,
                })
        });
        let elapsed_frame_seconds = match effect_instance.simulation_clock {
            EffectClock::Presentation => presentation_time.delta_secs(),
            EffectClock::ZooSimulation => zoo_simulation_time.delta_secs(),
        };
        emission_state.elapsed_seconds += elapsed_frame_seconds;
        if let Some((start_seconds, stop_seconds, frequency, phase)) = authored_emission_window {
            let authored_time_seconds = phase + emission_state.elapsed_seconds * frequency;
            effect_spawner.active =
                authored_time_seconds >= start_seconds && authored_time_seconds <= stop_seconds;
            if !effect_spawner.active {
                effect_spawner.spawn_count = 0;
                continue;
            }
        }
        if emission_state.manual_particle_count > 0 {
            effect_spawner.spawn_count = emission_state.manual_particle_count;
            emission_state.manual_particle_count = 0;
            continue;
        }
        let Some((birth_rate_curve_points, playback_is_looped)) =
            authored_emitter.and_then(|emitter| {
                emitter
                    .modifiers
                    .iter()
                    .find_map(|modifier| match modifier {
                        EffectModifier::BirthRateCurve { points, looped } => {
                            Some((points.as_slice(), *looped))
                        }
                        _ => None,
                    })
            })
        else {
            continue;
        };
        let curve_duration_seconds = birth_rate_curve_points
            .last()
            .map_or(0.0, |curve_point| curve_point.time);
        let curve_sample_time_seconds = if playback_is_looped && curve_duration_seconds > 0.0 {
            emission_state.elapsed_seconds % curve_duration_seconds
        } else {
            emission_state.elapsed_seconds.min(curve_duration_seconds)
        };
        let particle_birth_rate_per_second = birth_rate_curve_points
            .windows(2)
            .find(|adjacent_curve_points| {
                curve_sample_time_seconds >= adjacent_curve_points[0].time
                    && curve_sample_time_seconds <= adjacent_curve_points[1].time
            })
            .map_or_else(
                || {
                    birth_rate_curve_points
                        .last()
                        .map_or(0.0, |curve_point| curve_point.value[0])
                },
                |adjacent_curve_points| {
                    let curve_segment_width_seconds =
                        adjacent_curve_points[1].time - adjacent_curve_points[0].time;
                    let curve_segment_fraction = if curve_segment_width_seconds > 0.0 {
                        (curve_sample_time_seconds - adjacent_curve_points[0].time)
                            / curve_segment_width_seconds
                    } else {
                        1.0
                    };
                    adjacent_curve_points[0].value[0]
                        + (adjacent_curve_points[1].value[0] - adjacent_curve_points[0].value[0])
                            * curve_segment_fraction
                },
            );
        if !playback_is_looped && emission_state.elapsed_seconds > curve_duration_seconds {
            effect_spawner.spawn_count = 0;
        } else {
            emission_state.fractional_particle_remainder +=
                particle_birth_rate_per_second.max(0.0) * elapsed_frame_seconds;
            effect_spawner.spawn_count =
                emission_state.fractional_particle_remainder.floor() as u32;
            emission_state.fractional_particle_remainder -= effect_spawner.spawn_count as f32;
        }
    }
}

pub(super) fn select_effect_simulation_delta_time_from_authored_clock(
    presentation_time: Res<Time<Real>>,
    zoo_simulation_time: Res<Time<Virtual>>,
    mut effect_instances: Query<(
        &LiveParticleEffectInstance,
        &mut bevy_hanabi::prelude::EffectDeltaTime,
    )>,
) {
    for (effect_instance, mut effect_delta_time) in &mut effect_instances {
        effect_delta_time.0 = match effect_instance.simulation_clock {
            EffectClock::Presentation => presentation_time.delta_secs(),
            EffectClock::ZooSimulation => zoo_simulation_time.delta_secs(),
        };
    }
}

pub(super) fn retire_effect_instances_after_calculated_finite_lifetime(
    mut commands: Commands,
    presentation_time: Res<Time<Real>>,
    zoo_simulation_time: Res<Time<Virtual>>,
    mut effect_instances: Query<(
        Entity,
        &LiveParticleEffectInstance,
        &mut FiniteEffectInstanceLifetime,
    )>,
) {
    for (effect_entity, effect_instance, mut finite_lifetime) in &mut effect_instances {
        let elapsed_frame_duration = match effect_instance.simulation_clock {
            EffectClock::Presentation => presentation_time.delta(),
            EffectClock::ZooSimulation => zoo_simulation_time.delta(),
        };
        if finite_lifetime.0.tick(elapsed_frame_duration).is_finished() {
            commands.entity(effect_entity).despawn();
        }
    }
}
