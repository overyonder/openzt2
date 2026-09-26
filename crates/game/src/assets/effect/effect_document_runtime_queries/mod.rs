//! Queries derived from particle-effect documents for live execution.

use super::hanabi_custom_particle_modifiers::CollisionSpawnParticleInitialization;
use bevy::prelude::Vec3;

use openzt2_game_data::{
    particle::{
        authored_particle_system::{
            AuthoredParticleSpawnPolicy as SpawnPolicy,
            AuthoredParticleSystemDocument as EffectDocument,
            AuthoredParticleSystemModifier as EffectModifier,
            ParticleEffectSimulationClock as EffectClock,
        },
        ParticleEffectDocument as ParticleDocument,
    },
    AssetId,
};

pub(super) fn collect_all_effect_emitter_ids(particle_document: &ParticleDocument) -> Vec<AssetId> {
    match particle_document {
        ParticleDocument::LegacyNif(legacy_nif_effect) => vec![legacy_nif_effect.emitter_id],
        ParticleDocument::Effect(effect_document) => effect_document
            .emitters
            .iter()
            .map(|effect_emitter| effect_emitter.emitter_id)
            .collect(),
    }
}

pub(super) fn collect_root_effect_emitter_ids(
    particle_document: &ParticleDocument,
) -> Vec<AssetId> {
    match particle_document {
        ParticleDocument::LegacyNif(legacy_nif_effect) => vec![legacy_nif_effect.emitter_id],
        ParticleDocument::Effect(effect_document) => {
            let collision_spawn_child_emitter_ids = effect_document
                .emitters
                .iter()
                .flat_map(|effect_emitter| effect_emitter.modifiers.iter())
                .filter_map(|effect_modifier| match effect_modifier {
                    EffectModifier::SpawnOnPlane { target_emitter, .. } => Some(*target_emitter),
                    _ => None,
                })
                .collect::<Vec<_>>();
            effect_document
                .emitters
                .iter()
                .map(|effect_emitter| effect_emitter.emitter_id)
                .filter(|emitter_id| !collision_spawn_child_emitter_ids.contains(emitter_id))
                .collect()
        }
    }
}

pub(super) fn calculate_effect_emitter_tree_lifetime_seconds(
    particle_document: &ParticleDocument,
    emitter_id: AssetId,
) -> Option<f32> {
    match particle_document {
        ParticleDocument::LegacyNif(legacy_nif_effect) => Some(
            legacy_nif_effect
                .emit_stop_seconds
                .max(legacy_nif_effect.emit_start_seconds)
                + legacy_nif_effect.lifetime_seconds[1],
        ),
        ParticleDocument::Effect(effect_document) => {
            calculate_authored_particle_system_emitter_tree_lifetime_seconds(
                effect_document,
                emitter_id,
            )
        }
    }
}

fn calculate_authored_particle_system_emitter_tree_lifetime_seconds(
    effect_document: &EffectDocument,
    emitter_id: AssetId,
) -> Option<f32> {
    let effect_emitter = effect_document
        .emitters
        .iter()
        .find(|candidate_emitter| candidate_emitter.emitter_id == emitter_id)?;
    if effect_emitter.modifiers.iter().any(|effect_modifier| {
        matches!(
            effect_modifier,
            EffectModifier::DieWhenEmpty { enabled: false }
        )
    }) {
        return None;
    }
    let particle_lifetime_seconds = effect_emitter
        .modifiers
        .iter()
        .find_map(|effect_modifier| match effect_modifier {
            EffectModifier::LifetimeRange { max_seconds, .. } => Some(*max_seconds),
            EffectModifier::InitialPosition { lifetime_seconds } => Some(*lifetime_seconds),
            _ => None,
        })
        .unwrap_or(0.0);
    let emission_window_end_seconds =
        effect_emitter
            .modifiers
            .iter()
            .find_map(|effect_modifier| match effect_modifier {
                EffectModifier::EmitWindow {
                    stop_seconds,
                    frequency,
                    phase,
                    ..
                } if *frequency != 0.0 => Some(((*stop_seconds - *phase) / *frequency).max(0.0)),
                _ => None,
            });
    let emission_end_seconds = match effect_emitter.spawn {
        SpawnPolicy::Burst { .. } | SpawnPolicy::Once | SpawnPolicy::Manual => Some(0.0),
        SpawnPolicy::Curve { looped: false } => {
            effect_emitter
                .modifiers
                .iter()
                .find_map(|effect_modifier| match effect_modifier {
                    EffectModifier::BirthRateCurve { points, .. } => {
                        points.last().map(|curve_point| curve_point.time)
                    }
                    _ => None,
                })
        }
        SpawnPolicy::Rate { .. } | SpawnPolicy::Curve { looped: true } => {
            emission_window_end_seconds
        }
    }?;
    let mut descendant_emitter_lifetime_seconds = 0.0_f32;
    for target_emitter_id in effect_emitter
        .modifiers
        .iter()
        .filter_map(|effect_modifier| match effect_modifier {
            EffectModifier::SpawnOnPlane { target_emitter, .. } => Some(*target_emitter),
            _ => None,
        })
    {
        descendant_emitter_lifetime_seconds = descendant_emitter_lifetime_seconds.max(
            calculate_authored_particle_system_emitter_tree_lifetime_seconds(
                effect_document,
                target_emitter_id,
            )?,
        );
    }
    Some(emission_end_seconds + particle_lifetime_seconds + descendant_emitter_lifetime_seconds)
}

pub(super) fn select_effect_emitter_clock(
    particle_document: &ParticleDocument,
    emitter_id: AssetId,
) -> EffectClock {
    match particle_document {
        ParticleDocument::LegacyNif(_) => EffectClock::Presentation,
        ParticleDocument::Effect(effect_document) => effect_document
            .emitters
            .iter()
            .find(|candidate_emitter| candidate_emitter.emitter_id == emitter_id)
            .map_or(EffectClock::Presentation, |effect_emitter| {
                effect_emitter.clock
            }),
    }
}

pub(super) fn create_collision_spawn_initialization(
    parent_emitter_id: AssetId,
    modifier_index: usize,
    effect_modifier: &EffectModifier,
) -> Option<(AssetId, CollisionSpawnParticleInitialization)> {
    match effect_modifier {
        EffectModifier::SpawnOnPlane {
            target_emitter,
            radius,
            speed,
            velocity_out,
            velocity_up,
            velocity_direction,
            velocity_scale,
            offset,
            ..
        } => Some((
            *target_emitter,
            CollisionSpawnParticleInitialization {
                parent_emitter_id,
                modifier_index: u32::try_from(modifier_index).unwrap_or(u32::MAX),
                spawn_radius: *radius,
                initial_speed: *speed,
                outward_velocity: Vec3::from_array(*velocity_out),
                upward_velocity: Vec3::from_array(*velocity_up),
                base_velocity_direction: Vec3::from_array(*velocity_direction),
                velocity_scale: Vec3::from_array(*velocity_scale),
                position_offset: *offset,
            },
        )),
        _ => None,
    }
}
