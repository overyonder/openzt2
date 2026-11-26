use bevy::{diagnostic::FrameCount, prelude::*};
pub use bevy_hanabi::{EffectParticleSizeScale, ParticleAliveCount, ParticleSpawnBatch};
use bevy_hanabi::{EffectSpawner, ParticleSpawnPoint};
use openzt2_game_data::{particle::particle_emission_point::ParticleEmissionPoint, AssetId};

use super::{
    effect_runtime_types::{
        EffectRetirementReadback, ParticleEffectGraphReady, PendingEffectInstance,
    },
    ParticleEffectDocumentAsset,
};

#[derive(Component)]
#[require(ParticleAliveCount, ParticleSpawnBatch, EffectParticleSizeScale)]
pub struct ParticleEffectPointEmitter {
    effect_asset: Handle<ParticleEffectDocumentAsset>,
    emitter: AssetId,
    queued_points: Vec<ParticleEmissionPoint>,
    mesh: Option<Handle<Mesh>>,
    pending_manual_count: u32,
}

impl ParticleEffectPointEmitter {
    pub fn new(effect_asset: Handle<ParticleEffectDocumentAsset>, emitter: AssetId) -> Self {
        Self {
            effect_asset,
            emitter,
            queued_points: Vec::new(),
            mesh: None,
            pending_manual_count: 0,
        }
    }

    /// Emits one particle using the authored initializer and this mesh.
    pub fn with_single_particle_mesh(mut self, mesh: Handle<Mesh>) -> Self {
        self.mesh = Some(mesh);
        self.pending_manual_count = 1;
        self
    }

    /// Returns the live count from the current submission once all queued points are uploaded.
    pub fn sampled_live_count(
        &self,
        sample: &ParticleAliveCount,
        batch: &ParticleSpawnBatch,
    ) -> Option<u32> {
        if self.pending_manual_count > 0 || !self.queued_points.is_empty() {
            return None;
        }
        sample.sampled_count_after(batch.submission)
    }
}

#[derive(Message)]
pub struct AppendParticleEffectPoints {
    pub emitter: Entity,
    /// Positions and velocities in the emitter's local coordinate space.
    pub points: Box<[ParticleEmissionPoint]>,
}

#[derive(Component)]
pub(super) struct ParticlePointsAwaitingUpload;

#[derive(Component)]
pub(super) struct ParticleSpawnAwaitingClear;

pub(crate) fn prepare_explicit_particle_emitters(
    mut commands: Commands,
    emitters: Query<(Entity, &ParticleEffectPointEmitter), Added<ParticleEffectPointEmitter>>,
) {
    for (entity, emitter) in &emitters {
        let mut target = commands.entity(entity);
        target.insert(PendingEffectInstance {
            effect_asset: emitter.effect_asset.clone(),
            requested_emitter: Some(emitter.emitter),
            manual_particle_count: 0,
        });
        if let Some(mesh) = &emitter.mesh {
            target.insert((
                bevy_hanabi::EffectMesh(mesh.clone()),
                ParticlePointsAwaitingUpload,
            ));
        }
    }
}

pub(super) fn collect_requested_particle_points(
    mut commands: Commands,
    mut requests: MessageReader<AppendParticleEffectPoints>,
    mut emitters: Query<(
        &mut ParticleEffectPointEmitter,
        Has<ParticlePointsAwaitingUpload>,
    )>,
) {
    for request in requests.read() {
        if request.points.is_empty() {
            continue;
        }
        let Ok((mut emitter, upload_pending)) = emitters.get_mut(request.emitter) else {
            continue;
        };
        if !upload_pending && emitter.queued_points.is_empty() {
            commands
                .entity(request.emitter)
                .insert(ParticlePointsAwaitingUpload);
        }
        emitter.queued_points.extend_from_slice(&request.points);
    }
}

pub(super) fn clear_submitted_particle_points(
    mut commands: Commands,
    mut emitters: Query<
        (Entity, &mut ParticleSpawnBatch, &mut EffectSpawner),
        With<ParticleSpawnAwaitingClear>,
    >,
) {
    for (entity, mut batch, mut spawner) in &mut emitters {
        batch.points.clear();
        spawner.spawn_count = 0;
        commands
            .entity(entity)
            .remove::<ParticleSpawnAwaitingClear>();
    }
}

pub(super) fn upload_queued_particle_points(
    mut commands: Commands,
    frame: Res<FrameCount>,
    mut emitters: Query<
        (
            Entity,
            &mut ParticleEffectPointEmitter,
            &mut ParticleSpawnBatch,
            &mut EffectSpawner,
        ),
        (
            With<ParticlePointsAwaitingUpload>,
            With<ParticleEffectGraphReady>,
        ),
    >,
) {
    for (entity, mut emitter, mut batch, mut spawner) in &mut emitters {
        batch.submission = batch.submission.wrapping_add(1);
        batch.points.extend(
            emitter
                .queued_points
                .drain(..)
                .map(|point| ParticleSpawnPoint {
                    position: Vec3::from_array(point.position),
                    velocity: Vec3::from_array(point.velocity),
                }),
        );
        let manual_count = std::mem::take(&mut emitter.pending_manual_count);
        spawner.spawn_count = u32::try_from(batch.points.len())
            .expect("particle batch fits u32")
            .max(manual_count);
        let mut target = commands.entity(entity);
        target
            .remove::<ParticlePointsAwaitingUpload>()
            .insert(ParticleSpawnAwaitingClear);
        if manual_count != 0 {
            target.insert(EffectRetirementReadback::WaitingForParticles(
                frame.0.wrapping_add(1),
            ));
        }
    }
}
