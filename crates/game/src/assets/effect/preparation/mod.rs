//! Prepares a Hanabi effect from a particle document.

use bevy_hanabi::prelude::EffectAsset as HanabiEffectAsset;
use openzt2_game_data::{particle::ParticleEffectDocument as ParticleDocument, AssetId};

use super::{
    authored_particle_system_hanabi_effect_preparation::prepare_authored_particle_system_emitter_as_hanabi_effect_asset,
    hanabi_custom_particle_modifiers::CollisionSpawnParticleInitialization,
    legacy_nif_hanabi_effect_preparation::prepare_legacy_nif_particle_effect_as_hanabi_effect_asset,
};

pub(super) fn prepare_particle_document_as_hanabi_effect_asset(
    particle_document: &ParticleDocument,
    requested_emitter_id: AssetId,
    collision_spawn_initialization: Option<CollisionSpawnParticleInitialization>,
) -> Option<HanabiEffectAsset> {
    match particle_document {
        ParticleDocument::LegacyNif(legacy_nif_particle_effect) => {
            (legacy_nif_particle_effect.emitter_id == requested_emitter_id).then(|| {
                prepare_legacy_nif_particle_effect_as_hanabi_effect_asset(
                    legacy_nif_particle_effect,
                )
            })
        }
        ParticleDocument::Effect(authored_particle_system) => authored_particle_system
            .emitters
            .iter()
            .find(|authored_emitter| authored_emitter.emitter_id == requested_emitter_id)
            .map(|authored_particle_system_emitter| {
                prepare_authored_particle_system_emitter_as_hanabi_effect_asset(
                    authored_particle_system_emitter,
                    collision_spawn_initialization,
                )
            }),
    }
}
