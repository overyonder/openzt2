//! Emission timing, clocks, stopping, and GPU-backed retirement for live effects.

use bevy::prelude::*;
use bevy_hanabi::{
    CompiledParticleEffect, EffectDeltaTime, EffectParent, EffectSpawner, ParticleAliveCount,
    ParticleSpawnBatch,
};
use openzt2_game_data::particle::authored_particle_system::{
    AuthoredParticleCurvePoint, ParticleEffectSimulationClock as EffectClock,
};

use super::{
    effect_runtime_types::{
        ControlledEffectEmissionState, EffectRetirementReadback, ParticleEffectDocumentInstance,
        PendingEffectInstance,
    },
    explicit_particle_emission::ParticleEffectPointEmitter,
    LiveParticleEffectInstance, ParticleEffectDocumentAsset, StopParticleEffectsAttachedToEntity,
};

pub(super) fn align_attached_particle_effects_to_world_axes(
    attached: Query<
        (Entity, &ChildOf),
        With<super::effect_runtime_types::WorldAlignedParticleEffect>,
    >,
    mut transforms: ParamSet<(
        bevy::transform::helper::TransformHelper,
        Query<&mut Transform, With<super::effect_runtime_types::WorldAlignedParticleEffect>>,
    )>,
) {
    for (entity, parent) in &attached {
        let Ok(parent_transform) = transforms.p0().compute_global_transform(parent.parent()) else {
            continue;
        };
        let mut local_transforms = transforms.p1();
        if let Ok(mut transform) = local_transforms.get_mut(entity) {
            transform.rotation = parent_transform.compute_transform().rotation.inverse();
        }
    }
}

pub(super) fn stop_requested_particle_effect_children(
    mut commands: Commands,
    mut requests: MessageReader<StopParticleEffectsAttachedToEntity>,
    children: Query<&Children>,
    effects: Query<
        (
            Option<&ParticleEffectDocumentInstance>,
            Option<&LiveParticleEffectInstance>,
            Option<&PendingEffectInstance>,
        ),
        Or<(
            With<ParticleEffectDocumentInstance>,
            With<LiveParticleEffectInstance>,
            With<PendingEffectInstance>,
        )>,
    >,
) {
    for request in requests.read() {
        let Ok(children) = children.get(request.attached_entity) else {
            continue;
        };
        for effect in children.iter() {
            let Ok((document, live, pending)) = effects.get(effect) else {
                continue;
            };
            let document_id = document
                .map(|document| document.0.id())
                .or_else(|| live.map(|live| live.effect_asset.id()))
                .or_else(|| pending.map(|pending| pending.effect_asset.id()));
            if request
                .effect_asset
                .is_none_or(|id| document_id == Some(id))
            {
                commands.entity(effect).try_despawn();
            }
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
        let seconds = match effect_instance.simulation_clock {
            EffectClock::Presentation => presentation_time.delta_secs(),
            EffectClock::ZooSimulation => zoo_simulation_time.delta_secs(),
        };
        effect_delta_time.set_if_neq(EffectDeltaTime(seconds));
    }
}

pub(super) fn activate_authored_particle_spawners_for_emission_window(
    documents: Res<Assets<ParticleEffectDocumentAsset>>,
    mut emitters: Query<(
        &LiveParticleEffectInstance,
        &ControlledEffectEmissionState,
        &EffectDeltaTime,
        &CompiledParticleEffect,
        &InheritedVisibility,
        &mut EffectSpawner,
        Has<EffectParent>,
        Has<ParticleEffectPointEmitter>,
    )>,
) {
    for (live, timing, delta, compiled, visibility, mut spawner, collision, explicit) in
        &mut emitters
    {
        if !compiled.is_ready() || (!explicit && !visibility.get()) {
            continue;
        }
        let Some(prepared) = documents
            .get(&live.effect_asset)
            .and_then(|doc| doc.prepared_emitters.get(&live.emitter_id))
        else {
            spawner.active = false;
            continue;
        };
        let next_elapsed = timing.elapsed_seconds + delta.0;
        let active = !collision
            && !explicit
            && !prepared.manually_driven
            && prepared
                .emission_window
                .is_none_or(|window| window.contains(next_elapsed));
        if spawner.active != active {
            spawner.active = active;
        }
    }
}

pub(super) fn apply_authored_particle_emission_and_close_finished_spawners(
    mut commands: Commands,
    documents: Res<Assets<ParticleEffectDocumentAsset>>,
    mut emitters: Query<
        (
            Entity,
            &LiveParticleEffectInstance,
            &mut ControlledEffectEmissionState,
            &EffectDeltaTime,
            &CompiledParticleEffect,
            &InheritedVisibility,
            &mut EffectSpawner,
            Has<EffectParent>,
            Has<EffectRetirementReadback>,
        ),
        Without<ParticleEffectPointEmitter>,
    >,
) {
    for (entity, live, mut timing, delta, compiled, visibility, mut spawner, collision, draining) in
        &mut emitters
    {
        if !compiled.is_ready() || !visibility.get() || collision || draining {
            spawner.spawn_count = 0;
            continue;
        }
        let Some(document) = documents.get(&live.effect_asset) else {
            continue;
        };
        let Some(prepared) = document.prepared_emitters.get(&live.emitter_id) else {
            continue;
        };
        timing.elapsed_seconds += delta.0;
        let elapsed = timing.elapsed_seconds;
        let window_closed = prepared
            .emission_window
            .is_some_and(|window| window.has_ended(elapsed));
        let within_window = prepared
            .emission_window
            .is_none_or(|window| window.contains(elapsed));
        let mut emission_closed = window_closed;
        if within_window && timing.manual_particle_count > 0 {
            spawner.spawn_count = std::mem::take(&mut timing.manual_particle_count);
        } else if let Some((points, looped)) = prepared
            .birth_rate_curve
            .and_then(|curve| curve.points(&document.particle_document))
        {
            let duration = points.last().map_or(0.0, |point| point.time);
            let curve_ended = !looped && elapsed > duration;
            emission_closed |= curve_ended;
            if within_window && !curve_ended {
                let sample = if looped && duration > 0.0 {
                    elapsed % duration
                } else {
                    elapsed.min(duration)
                };
                timing.fractional_particle_remainder +=
                    sample_birth_rate(points, sample).max(0.0) * delta.0;
                spawner.spawn_count = timing.fractional_particle_remainder.floor() as u32;
                timing.fractional_particle_remainder -= spawner.spawn_count as f32;
            } else {
                spawner.spawn_count = 0;
            }
        } else if prepared.manually_driven {
            emission_closed |= within_window && timing.manual_particle_count == 0;
        }
        if !prepared.manually_driven {
            emission_closed |= spawner.has_completed() && timing.manual_particle_count == 0;
        }
        if prepared.retires_when_empty && emission_closed {
            commands
                .entity(entity)
                .insert(EffectRetirementReadback::WaitingForParticles);
        }
    }
}

fn sample_birth_rate(points: &[AuthoredParticleCurvePoint], time: f32) -> f32 {
    points
        .windows(2)
        .find(|pair| time >= pair[0].time && time <= pair[1].time)
        .map_or_else(
            || points.last().map_or(0.0, |point| point.value[0]),
            |pair| {
                let width = pair[1].time - pair[0].time;
                let fraction = if width > 0.0 {
                    (time - pair[0].time) / width
                } else {
                    1.0
                };
                pair[0].value[0] + (pair[1].value[0] - pair[0].value[0]) * fraction
            },
        )
}

pub(super) fn confirm_particle_readbacks_before_draining_collision_children(
    mut commands: Commands,
    mut readbacks: Query<(
        Entity,
        &ParticleAliveCount,
        Option<&ParticleSpawnBatch>,
        Option<&Children>,
        &mut EffectRetirementReadback,
    )>,
    collision_parents: Query<&EffectParent>,
) {
    for (entity, count, batch, children, mut retirement) in &mut readbacks {
        if *retirement != EffectRetirementReadback::WaitingForParticles
            || count.sampled_count_after(batch.map_or(0, |batch| batch.submission)) != Some(0)
        {
            continue;
        }
        // Collision events can be consumed on the next GPU step. Request a
        // fresh readback from each child only after this emitter reaches zero.
        for child in children.into_iter().flat_map(|children| children.iter()) {
            if collision_parents
                .get(child)
                .is_ok_and(|parent| parent.entity == entity)
            {
                commands
                    .entity(child)
                    .insert(EffectRetirementReadback::WaitingForParticles);
            }
        }
        *retirement = EffectRetirementReadback::WaitingForChildren;
    }
}

pub(super) fn retire_drained_particle_effect_trees(
    mut commands: Commands,
    mut retirements: Query<(
        Entity,
        &mut EffectRetirementReadback,
        Option<&Children>,
        Has<EffectParent>,
    )>,
    collision_parents: Query<&EffectParent>,
) {
    let drained = retirements
        .iter()
        .filter_map(|(entity, retirement, children, collision)| {
            if *retirement != EffectRetirementReadback::WaitingForChildren {
                return None;
            }
            let children_are_drained = children
                .into_iter()
                .flat_map(|children| children.iter())
                .filter(|&child| {
                    collision_parents
                        .get(child)
                        .is_ok_and(|parent| parent.entity == entity)
                })
                .all(|child| {
                    retirements.get(child).is_ok_and(|(_, retirement, ..)| {
                        *retirement == EffectRetirementReadback::Drained
                    })
                });
            children_are_drained.then_some((entity, collision))
        })
        .collect::<Vec<_>>();
    for (entity, collision) in drained {
        if collision {
            if let Ok((_, mut retirement, ..)) = retirements.get_mut(entity) {
                *retirement = EffectRetirementReadback::Drained;
            }
        } else {
            commands.entity(entity).try_despawn();
        }
    }
}

pub(super) fn retire_empty_particle_effect_document_roots(
    mut commands: Commands,
    finished: Query<
        Entity,
        (
            With<ParticleEffectDocumentInstance>,
            Without<PendingEffectInstance>,
            Without<Children>,
        ),
    >,
) {
    for document in &finished {
        commands.entity(document).try_despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parent_with_no_gpu_readback_leaves_its_collision_children_available() {
        let mut app = App::new();
        app.add_systems(
            Update,
            (
                confirm_particle_readbacks_before_draining_collision_children,
                retire_drained_particle_effect_trees,
            )
                .chain(),
        );
        let parent = app
            .world_mut()
            .spawn(EffectRetirementReadback::WaitingForParticles)
            .id();
        let child = app
            .world_mut()
            .spawn((ChildOf(parent), EffectParent::new(parent)))
            .id();
        app.update();
        assert!(app.world().get_entity(parent).is_ok());
        assert!(app.world().get_entity(child).is_ok());
        assert!(app.world().get::<EffectRetirementReadback>(child).is_none());
    }
}
