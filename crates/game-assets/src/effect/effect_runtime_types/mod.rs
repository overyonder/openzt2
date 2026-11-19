//! Private Bevy runtime state for loading and running particle-effect instances.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::ParticleEffectDocumentAsset;

#[derive(Component, Clone)]
pub(super) struct PendingEffectInstance {
    pub(super) effect_asset: Handle<ParticleEffectDocumentAsset>,
    pub(super) requested_emitter: Option<AssetId>,
    pub(super) manual_particle_count: u32,
}

/// Identity retained on a complete document root, including multi-emitter roots.
#[derive(Component)]
pub(super) struct ParticleEffectDocumentInstance(pub(super) Handle<ParticleEffectDocumentAsset>);

#[derive(Component, Default, PartialEq)]
#[require(bevy_hanabi::ParticleAliveCount)]
pub(super) enum EffectRetirementReadback {
    #[default]
    WaitingForParticles,
    WaitingForChildren,
    Drained,
}

#[derive(Component)]
pub(super) struct ParticleEffectGraphReady;

#[derive(Component, Clone, Debug)]
pub(super) struct ControlledEffectEmissionState {
    pub(super) elapsed_seconds: f32,
    pub(super) fractional_particle_remainder: f32,
    pub(super) manual_particle_count: u32,
}

/// Attached emitters whose orientation stays aligned with the world axes.
#[derive(Component)]
pub(super) struct WorldAlignedParticleEffect;
