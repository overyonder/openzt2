use bevy::prelude::*;
use bevy_hanabi::{ParticleAliveCount, ParticleSpawnBatch, ParticleSpawnPoint};
use openzt2_game_data::{particle::particle_emission_point::ParticleEmissionPoint, AssetId};

use super::{
    effect_runtime_types::PendingEffectInstance, LiveParticleEffectInstance,
    ParticleEffectDocumentAsset,
};

#[derive(Component)]
pub struct ParticleEffectPointEmitter {
    effect_asset: Handle<ParticleEffectDocumentAsset>,
    emitter: AssetId,
    queued_points: Vec<ParticleEmissionPoint>,
    alive_count: ParticleAliveCount,
    submission: u32,
    mesh: Option<Handle<Mesh>>,
    pending_manual_count: u32,
    size_scale: f32,
}

impl ParticleEffectPointEmitter {
    pub fn new(effect_asset: Handle<ParticleEffectDocumentAsset>, emitter: AssetId) -> Self {
        Self {
            effect_asset,
            emitter,
            queued_points: Vec::new(),
            alive_count: ParticleAliveCount::default(),
            submission: 0,
            mesh: None,
            pending_manual_count: 0,
            size_scale: 1.0,
        }
    }

    /// Emits one particle using the authored initializer and this mesh.
    pub fn with_single_particle_mesh(mut self, mesh: Handle<Mesh>) -> Self {
        self.mesh = Some(mesh);
        self.pending_manual_count = 1;
        self
    }

    /// Latest completed GPU sample; unavailable until the emitter has rendered.
    pub fn sampled_live_count(&self) -> Option<u32> {
        if self.pending_manual_count > 0 || !self.queued_points.is_empty() {
            return None;
        }
        self.alive_count.sampled_count_after(self.submission)
    }

    pub fn set_size_scale(&mut self, scale: f32) {
        assert!(scale.is_finite() && scale >= 0.0);
        self.size_scale = scale;
    }
}

#[derive(Message)]
pub struct AppendParticleEffectPoints {
    pub emitter: Entity,
    /// Positions and velocities in the emitter's local coordinate space.
    pub points: Box<[ParticleEmissionPoint]>,
}

pub(crate) fn prepare_explicit_particle_emitters(
    mut commands: Commands,
    emitters: Query<(Entity, &ParticleEffectPointEmitter), Added<ParticleEffectPointEmitter>>,
) {
    for (entity, emitter) in &emitters {
        commands.entity(entity).insert((
            PendingEffectInstance {
                effect_asset: emitter.effect_asset.clone(),
                requested_emitter: Some(emitter.emitter),
                manual_particle_count: 0,
                collision_spawn_context: None,
            },
            emitter.alive_count.clone(),
            ParticleSpawnBatch::default(),
            bevy_hanabi::EffectParticleSizeScale(emitter.size_scale),
        ));
        if let Some(mesh) = &emitter.mesh {
            commands
                .entity(entity)
                .insert(bevy_hanabi::EffectMesh(mesh.clone()));
        }
    }
}

pub(crate) fn upload_explicit_particle_emission_points(
    mut requests: MessageReader<AppendParticleEffectPoints>,
    mut emitters: Query<(
        &mut ParticleEffectPointEmitter,
        Option<&bevy_hanabi::CompiledParticleEffect>,
        Option<&mut ParticleSpawnBatch>,
        Option<&mut bevy_hanabi::EffectSpawner>,
        Option<&mut bevy_hanabi::EffectParticleSizeScale>,
    )>,
) {
    for request in requests.read() {
        if let Ok((mut emitter, ..)) = emitters.get_mut(request.emitter) {
            emitter.queued_points.extend_from_slice(&request.points);
        }
    }
    for (mut emitter, compiled, batch, spawner, size_scale) in &mut emitters {
        if let Some(mut size_scale) = size_scale {
            size_scale.0 = emitter.size_scale;
        }
        let (Some(mut batch), Some(mut spawner)) = (batch, spawner) else {
            continue;
        };
        batch.points.clear();
        spawner.spawn_count = 0;
        if !compiled.is_some_and(|compiled| compiled.is_ready()) {
            continue;
        }
        if !emitter.queued_points.is_empty() || emitter.pending_manual_count > 0 {
            emitter.submission = emitter.submission.wrapping_add(1);
            batch.submission = emitter.submission;
        }
        batch.points.extend(
            emitter
                .queued_points
                .drain(..)
                .map(|point| ParticleSpawnPoint {
                    position: Vec3::from_array(point.position),
                    velocity: Vec3::from_array(point.velocity),
                }),
        );
        spawner.spawn_count = u32::try_from(batch.points.len()).expect("particle batch fits u32");
        spawner.spawn_count = spawner
            .spawn_count
            .max(std::mem::take(&mut emitter.pending_manual_count));
    }
}

pub(crate) fn retain_explicit_particle_emitters(
    mut commands: Commands,
    emitters: Query<
        Entity,
        (
            With<ParticleEffectPointEmitter>,
            Added<LiveParticleEffectInstance>,
        ),
    >,
) {
    for emitter in &emitters {
        commands
            .entity(emitter)
            .remove::<super::effect_runtime_types::FiniteEffectInstanceLifetime>();
    }
}
