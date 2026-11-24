use std::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex,
};

use bevy::{ecs::change_detection::Tick, prelude::*};

use crate::CompiledParticleEffect;

/// Whether the render world can simulate and draw this compiled effect.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParticleEffectGpuReady(pub(crate) bool);

impl ParticleEffectGpuReady {
    /// Returns whether the current compilation is ready to use.
    pub fn is_ready(&self) -> bool {
        self.0
    }
}

/// Opt-in GPU particle count. The first value is published after a readback completes.
#[derive(Component, Debug, Default, PartialEq, Eq)]
pub struct ParticleAliveCount(Option<ParticleCountSample>);

impl ParticleAliveCount {
    /// Returns the live count for the specified submission, if sampled.
    pub fn sampled_count_after(&self, submission: u32) -> Option<u32> {
        self.0
            .filter(|sample| sample.submission == submission)
            .map(|sample| sample.count)
    }

    /// Checks for zero live particles and queued GPU births from a sufficiently recent frame.
    pub fn is_drained_after(&self, submission: u32, earliest_frame: u32) -> bool {
        self.0.is_some_and(|sample| {
            sample.submission == submission
                && sample.frame.wrapping_sub(earliest_frame) < (1 << 31)
                && sample.count == 0
                && !sample.queued
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ParticleCountSample {
    pub(crate) submission: u32,
    pub(crate) frame: u32,
    pub(crate) count: u32,
    pub(crate) queued: bool,
}

pub(crate) enum ParticleRenderFeedback {
    Readiness {
        entity: Entity,
        compilation: Tick,
        ready: bool,
    },
    Count {
        entity: Entity,
        request: Tick,
        sample: ParticleCountSample,
    },
}

#[derive(Resource)]
pub(crate) struct ParticleRenderFeedbackSender(pub(crate) Sender<ParticleRenderFeedback>);

#[derive(Resource)]
pub(crate) struct ParticleRenderFeedbackReceiver(Mutex<Receiver<ParticleRenderFeedback>>);

pub(crate) fn particle_render_feedback_channel(
) -> (ParticleRenderFeedbackSender, ParticleRenderFeedbackReceiver) {
    let (sender, receiver) = mpsc::channel();
    (
        ParticleRenderFeedbackSender(sender),
        ParticleRenderFeedbackReceiver(Mutex::new(receiver)),
    )
}

pub(crate) fn receive_particle_render_feedback(
    feedback: Res<ParticleRenderFeedbackReceiver>,
    mut readiness: Query<(Ref<CompiledParticleEffect>, &mut ParticleEffectGpuReady)>,
    mut counts: Query<&mut ParticleAliveCount>,
) {
    for message in feedback.0.lock().unwrap().try_iter() {
        match message {
            ParticleRenderFeedback::Readiness {
                entity,
                compilation,
                ready,
            } => {
                if let Ok((compiled, mut status)) = readiness.get_mut(entity) {
                    if compilation == compiled.last_changed() {
                        status.set_if_neq(ParticleEffectGpuReady(ready));
                    }
                }
            }
            ParticleRenderFeedback::Count {
                entity,
                request,
                sample,
            } => {
                if let Ok(mut count) = counts.get_mut(entity) {
                    if count.added() == request
                        && count.0.is_none_or(|previous| {
                            sample.frame.wrapping_sub(previous.frame) < (1 << 31)
                        })
                    {
                        count.set_if_neq(ParticleAliveCount(Some(sample)));
                    }
                }
            }
        }
    }
}
