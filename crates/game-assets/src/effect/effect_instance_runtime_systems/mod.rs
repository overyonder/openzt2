//! Emission timing, clocks, stopping, and GPU-backed retirement for live effects.

use bevy::{
    diagnostic::FrameCount, platform::collections::HashSet, prelude::*,
    transform::helper::TransformHelper,
};
use bevy_hanabi::{
    EffectChildren, EffectDeltaTime, EffectParent, EffectSpawner, ParticleAliveCount,
    ParticleEffectGpuReady, ParticleSpawnBatch, Random,
};
use openzt2_game_data::particle::authored_particle_system::{
    AuthoredParticleCurvePoint, ParticleEffectSimulationClock as EffectClock,
};

use super::{
    effect_runtime_types::{
        ControlledEffectEmissionSettings, ControlledEffectEmissionState, EffectRetirementReadback,
        ParticleChildrenMayHaveDrained, ParticleEffectDocumentInstance, ParticleEffectGraphReady,
        PendingEffectInstance, WorldAlignedParticleEffect,
    },
    explicit_particle_emission::ParticleEffectPointEmitter,
    LiveParticleEffectInstance, StopParticleEffectsAttachedToEntity,
};

pub(super) fn align_attached_particle_effects_to_world_axes(
    attached: Query<&ChildOf, With<WorldAlignedParticleEffect>>,
    children: Query<&Children>,
    mut removed_parents: RemovedComponents<ChildOf>,
    mut transforms: ParamSet<(
        (
            Query<(Ref<Transform>, Option<Ref<ChildOf>>)>,
            Query<
                (),
                (
                    With<WorldAlignedParticleEffect>,
                    Or<(
                        Added<WorldAlignedParticleEffect>,
                        Changed<Transform>,
                        Changed<ChildOf>,
                    )>,
                ),
            >,
            TransformHelper,
        ),
        Query<&mut Transform, With<WorldAlignedParticleEffect>>,
    )>,
) {
    let orphans: HashSet<_> = removed_parents.read().collect();
    let mut parents = HashSet::new();
    for attachment in &attached {
        let parent = attachment.parent();
        if !parents.insert(parent) {
            continue;
        }
        let Ok(siblings) = children.get(parent) else {
            continue;
        };

        let rotation = {
            let (local_transforms, changed_effects, transform_helper) = transforms.p0();
            let mut ancestor = Some(parent);
            let mut parent_changed = false;
            while let Some(entity) = ancestor {
                let Ok((transform, relationship)) = local_transforms.get(entity) else {
                    break;
                };
                parent_changed = transform.is_changed()
                    || relationship.as_ref().is_some_and(DetectChanges::is_changed)
                    || orphans.contains(&entity);
                if parent_changed {
                    break;
                }
                ancestor = relationship.map(|relationship| relationship.parent());
            }
            if !parent_changed && !siblings.iter().any(|child| changed_effects.contains(child)) {
                continue;
            }
            let Ok(parent_transform) = transform_helper.compute_global_transform(parent) else {
                continue;
            };
            parent_transform.compute_transform().rotation.inverse()
        };
        let mut local_transforms = transforms.p1();
        let mut siblings = local_transforms.iter_many_mut(siblings.iter()).matched();
        while let Some(mut local_transform) = siblings.fetch_next() {
            if local_transform.rotation != rotation {
                local_transform.rotation = rotation;
            }
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

pub(super) fn publish_ready_particle_effect_graphs(
    mut commands: Commands,
    changed: Query<
        Entity,
        (
            With<LiveParticleEffectInstance>,
            Or<(
                Changed<ParticleEffectGpuReady>,
                Changed<EffectChildren>,
                Added<LiveParticleEffectInstance>,
            )>,
        ),
    >,
    mut removed_children: RemovedComponents<EffectChildren>,
    roots: Query<
        Has<ParticleEffectGraphReady>,
        (With<LiveParticleEffectInstance>, Without<EffectParent>),
    >,
    parents: Query<&EffectParent>,
    emitters: Query<(&ParticleEffectGpuReady, Option<&EffectChildren>)>,
) {
    let mut changed_roots = HashSet::new();
    for emitter in changed.iter().chain(removed_children.read()) {
        changed_roots.insert(parents.iter_ancestors(emitter).last().unwrap_or(emitter));
    }
    let mut pending = Vec::new();
    for root in changed_roots {
        let Ok(published) = roots.get(root) else {
            continue;
        };
        pending.clear();
        pending.push(root);
        let mut ready = true;
        while let Some(emitter) = pending.pop() {
            let Ok((status, children)) = emitters.get(emitter) else {
                ready = false;
                break;
            };
            if !status.is_ready() {
                ready = false;
                break;
            }
            pending.extend(children.into_iter().flat_map(RelationshipTarget::iter));
        }
        if ready && !published {
            commands.entity(root).insert(ParticleEffectGraphReady);
        } else if !ready && published {
            commands.entity(root).remove::<ParticleEffectGraphReady>();
        }
    }
}

pub(super) fn tick_authored_particle_emitters(
    mut commands: Commands,
    frame: Res<FrameCount>,
    presentation_time: Res<Time<Real>>,
    zoo_time: Res<Time<Virtual>>,
    mut random: ResMut<Random>,
    mut emitters: Query<(
        Entity,
        &ControlledEffectEmissionSettings,
        &mut ControlledEffectEmissionState,
        &mut EffectDeltaTime,
        &ParticleEffectGpuReady,
        &InheritedVisibility,
        &mut EffectSpawner,
        Has<EffectParent>,
        Has<ParticleEffectGraphReady>,
        Has<EffectRetirementReadback>,
        Has<ParticleEffectPointEmitter>,
    )>,
) {
    for (
        entity,
        settings,
        mut timing,
        mut effect_delta,
        status,
        visibility,
        mut spawner,
        collision,
        ready,
        draining,
        explicit,
    ) in &mut emitters
    {
        let delta = match settings.clock {
            EffectClock::Presentation => presentation_time.delta(),
            EffectClock::ZooSimulation => zoo_time.delta(),
        };
        effect_delta.set_if_neq(EffectDeltaTime(delta.as_secs_f32()));
        if explicit {
            continue;
        }
        if collision || !ready || !status.is_ready() || !visibility.get() || draining {
            if spawner.active || spawner.spawn_count != 0 {
                spawner.active = false;
                spawner.spawn_count = 0;
            }
            continue;
        }

        let start = timing.elapsed.elapsed_secs();
        let end = timing.elapsed.tick(delta).elapsed_secs();
        let (within_window, mut emission_closed) = settings
            .emission_window
            .map_or((true, false), |window| window.sample(end));
        let active = !settings.manually_driven && within_window;
        if spawner.active != active {
            spawner.active = active;
        }
        let mut spawn_count = if active {
            spawner.tick(delta.as_secs_f32(), &mut random.0)
        } else {
            0
        };
        if within_window && timing.manual_particle_count > 0 {
            spawn_count = std::mem::take(&mut timing.manual_particle_count);
        } else if let Some((points, looped)) = &settings.birth_rate_curve {
            let duration = points.last().map_or(0.0, |point| point.time);
            emission_closed |= !*looped && end > duration;
            let (start, end) = settings
                .emission_window
                .map_or((start, end), |window| window.active_interval(start, end));
            timing.fractional_particle_remainder +=
                integrate_birth_rate(points, *looped, start, end);
            spawn_count = timing.fractional_particle_remainder.floor() as u32;
            timing.fractional_particle_remainder -= spawn_count as f32;
        } else if settings.manually_driven {
            emission_closed |= within_window;
        }
        if !settings.manually_driven {
            emission_closed |= spawner.has_completed() && timing.manual_particle_count == 0;
        }
        if spawner.spawn_count != spawn_count {
            spawner.spawn_count = spawn_count;
        }
        if settings.retires_when_empty && emission_closed {
            commands
                .entity(entity)
                .insert(EffectRetirementReadback::WaitingForParticles(
                    frame.0.wrapping_add(1),
                ));
        }
    }
}

fn integrate_birth_rate(
    points: &[AuthoredParticleCurvePoint],
    looped: bool,
    start: f32,
    end: f32,
) -> f32 {
    let Some(last) = points.last() else {
        return 0.0;
    };
    if end <= start {
        return 0.0;
    }
    let duration = last.time;
    if duration <= 0.0 {
        return if looped {
            last.value[0].max(0.0) * (end - start)
        } else {
            0.0
        };
    }
    if !looped {
        return integrate_birth_rate_segments(points, start, end.min(duration));
    }

    let first_cycle = (start / duration).floor();
    let last_cycle = (end / duration).floor();
    let first_sample = start % duration;
    let last_sample = end % duration;
    if first_cycle == last_cycle {
        integrate_birth_rate_segments(points, first_sample, last_sample)
    } else {
        integrate_birth_rate_segments(points, first_sample, duration)
            + (last_cycle - first_cycle - 1.0)
                * integrate_birth_rate_segments(points, 0.0, duration)
            + integrate_birth_rate_segments(points, 0.0, last_sample)
    }
}

fn integrate_birth_rate_segments(
    points: &[AuthoredParticleCurvePoint],
    start: f32,
    end: f32,
) -> f32 {
    if end <= start {
        return 0.0;
    }
    let first = &points[0];
    let before_first = (end.min(first.time) - start).max(0.0) * first.value[0].max(0.0);
    points.windows(2).fold(before_first, |area, keys| {
        let left = &keys[0];
        let right = &keys[1];
        let segment_start = start.max(left.time);
        let segment_end = end.min(right.time);
        let width = segment_end - segment_start;
        if width <= 0.0 {
            return area;
        }
        let slope = (right.value[0] - left.value[0]) / (right.time - left.time);
        let a = slope.mul_add(segment_start - left.time, left.value[0]);
        let b = slope.mul_add(segment_end - left.time, left.value[0]);
        let high = a.max(b);
        let low = a.min(b);
        area + if high <= 0.0 {
            0.0
        } else if low >= 0.0 {
            (high + low) * width * 0.5
        } else {
            high * width * (high / (high - low)) * 0.5
        }
    })
}

pub(super) fn confirm_particle_readbacks_before_draining_collision_children(
    mut commands: Commands,
    frame: Res<FrameCount>,
    mut retirements: ParamSet<(
        Query<
            (
                Entity,
                &ParticleAliveCount,
                Option<&ParticleSpawnBatch>,
                &EffectRetirementReadback,
            ),
            Or<(
                Changed<ParticleAliveCount>,
                Changed<EffectRetirementReadback>,
            )>,
        >,
        Query<&mut EffectRetirementReadback>,
    )>,
    children: Query<&EffectChildren>,
) {
    let drained_emitters: Vec<_> = retirements
        .p0()
        .iter()
        .filter_map(|(entity, count, batch, status)| {
            let EffectRetirementReadback::WaitingForParticles(earliest) = *status else {
                return None;
            };
            count
                .is_drained_after(batch.map_or(0, |batch| batch.submission), earliest)
                .then_some(entity)
        })
        .collect();
    let mut states = retirements.p1();
    for entity in drained_emitters {
        let Ok(mut retirement) = states.get_mut(entity) else {
            continue;
        };
        *retirement = EffectRetirementReadback::WaitingForChildren;
        for child in children
            .get(entity)
            .ok()
            .into_iter()
            .flat_map(RelationshipTarget::iter)
        {
            let waiting = EffectRetirementReadback::WaitingForParticles(frame.0.wrapping_add(1));
            if let Ok(mut retirement) = states.get_mut(child) {
                retirement.set_if_neq(waiting);
            } else {
                commands.entity(child).insert(waiting);
            }
        }
        commands.trigger(ParticleChildrenMayHaveDrained { entity });
    }
}

pub(super) fn retire_drained_particle_effect_tree(
    trigger: On<ParticleChildrenMayHaveDrained>,
    mut commands: Commands,
    mut retirements: Query<&mut EffectRetirementReadback>,
    children: Query<&EffectChildren>,
    parents: Query<&EffectParent>,
) {
    let mut entity = trigger.entity;
    loop {
        if !retirements
            .get(entity)
            .is_ok_and(|retirement| *retirement == EffectRetirementReadback::WaitingForChildren)
            || children
                .get(entity)
                .ok()
                .into_iter()
                .flat_map(RelationshipTarget::iter)
                .any(|child| {
                    !retirements
                        .get(child)
                        .is_ok_and(|retirement| *retirement == EffectRetirementReadback::Drained)
                })
        {
            break;
        }
        let Ok(parent) = parents.get(entity) else {
            commands.entity(entity).try_despawn();
            break;
        };
        if let Ok(mut retirement) = retirements.get_mut(entity) {
            *retirement = EffectRetirementReadback::Drained;
        }
        entity = parent.entity;
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
        app.init_resource::<FrameCount>()
            .add_observer(retire_drained_particle_effect_tree)
            .add_systems(
                Update,
                confirm_particle_readbacks_before_draining_collision_children,
            );
        let parent = app
            .world_mut()
            .spawn(EffectRetirementReadback::WaitingForParticles(0))
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
