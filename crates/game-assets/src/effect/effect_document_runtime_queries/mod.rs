//! Queries derived from particle-effect documents for live execution.

use super::hanabi_custom_particle_modifiers::CollisionSpawnParticleInitialization;
use bevy::prelude::Vec3;

use openzt2_game_data::{
    particle::{
        authored_particle_system::AuthoredParticleSystemModifier as EffectModifier,
        ParticleEffectDocument as ParticleDocument,
    },
    AssetId,
};

pub fn collect_root_effect_emitter_ids(particle_document: &ParticleDocument) -> Vec<AssetId> {
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

pub(super) fn create_collision_spawn_initialization(
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
                spawn_radius: *radius,
                initial_speed: *speed,
                outward_velocity: Vec3::from_array(crate::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates(*velocity_out)),
                upward_velocity: Vec3::from_array(crate::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates(*velocity_up)),
                base_velocity_direction: Vec3::from_array(crate::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates(*velocity_direction)),
                velocity_scale: Vec3::new(velocity_scale[0], velocity_scale[2], velocity_scale[1]),
                position_offset: *offset,
            },
        )),
        _ => None,
    }
}
