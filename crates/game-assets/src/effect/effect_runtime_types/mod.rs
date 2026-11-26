//! Private Bevy runtime state for loading and running particle-effect instances.

use std::sync::Arc;

use bevy::{prelude::*, time::Stopwatch};
use openzt2_game_data::{
    particle::authored_particle_system::{
        AuthoredParticleCurvePoint, ParticleEffectSimulationClock,
    },
    scene_prefab::PrefabControllerClock,
    AssetId,
};

use super::{prepared_effect_emitters::ParticleEmissionWindow, ParticleEffectDocumentAsset};

#[derive(Component, Clone, PartialEq)]
pub(super) struct PendingEffectInstance {
    pub(super) effect_asset: Handle<ParticleEffectDocumentAsset>,
    pub(super) requested_emitter: Option<AssetId>,
    pub(super) manual_particle_count: u32,
}

/// Identity retained on a complete document root, including multi-emitter roots.
#[derive(Component)]
pub(super) struct ParticleEffectDocumentInstance(pub(super) Handle<ParticleEffectDocumentAsset>);

#[derive(Component, PartialEq)]
#[require(bevy_hanabi::ParticleAliveCount)]
pub(super) enum EffectRetirementReadback {
    WaitingForParticles(u32),
    WaitingForChildren,
    Drained,
}

#[derive(EntityEvent)]
pub(super) struct ParticleChildrenMayHaveDrained {
    pub(super) entity: Entity,
}

#[derive(Component)]
pub(super) struct ParticleEffectGraphReady;

#[derive(Component, Clone, Debug)]
pub(super) struct ControlledEffectEmissionState {
    pub(super) elapsed: Stopwatch,
    pub(super) fractional_particle_remainder: f32,
    pub(super) manual_particle_count: u32,
}

#[derive(Component, Clone, Debug, PartialEq)]
pub(super) struct ControlledEffectEmissionSettings {
    pub(super) clock: ParticleEffectSimulationClock,
    pub(super) retires_when_empty: bool,
    pub(super) emission_window: Option<ParticleEmissionWindow>,
    pub(super) controller_clock: Option<PrefabControllerClock>,
    pub(super) birth_rate_curve: Option<(Arc<[AuthoredParticleCurvePoint]>, bool)>,
    pub(super) manually_driven: bool,
}

/// Attached emitters whose orientation stays aligned with the world axes.
#[derive(Component)]
pub(super) struct WorldAlignedParticleEffect;
