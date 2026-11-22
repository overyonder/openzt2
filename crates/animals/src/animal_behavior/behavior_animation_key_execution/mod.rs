use avian3d::prelude::{CollisionStart, LinearVelocity, RigidBody, Sleeping};
use bevy::{ecs::system::SystemParam, platform::collections::HashSet, prelude::*};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::{
        WorldDefinitions, WorldDefinitionsView,
    },
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::{
    animation::animation_text_key::AuthoredAnimationTextAction,
    world_definitions::world_objects::WorldObjectWhapPhysics, AssetId,
};
use openzt2_world_state::{
    animation_graph::{
        animation_graph_playback_message_types::{
            AnimationClipPlaybackRequest, CancelAnimationClipPlaybackRequest,
        },
        animation_presentation_relationship_types::AnimationPresentationOwner,
    },
    animation_playback::{
        animation_event_message_types::{
            AnimationAiCommandRequested, AnimationObjectCommand, AnimationObjectCommandDelivery,
            AnimationWhapRequested,
        },
        animation_joint_target_types::{AnimationJointTarget, AnimationRigJoints},
        animation_playback_controller_types::{
            AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
        },
    },
    application_schedule::FixedGameSet,
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::{DefinitionId, WorldMember},
};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    activated_behavior_actor, behavior_action_actors, update_waiting_behavior_actors,
};
use openzt2_zoo_state::{
    animal_behavior::behavior_random_stream_state::BehaviorRandomStream,
    animal_health::types::{AnimalDied, Dead, DeathCause},
    animal_lifecycle::types::Animal,
    behavior_task_execution_types::{
        continuations::BehaviorContinuationFrames,
        running_actions::{
            BehaviorAnimationEffect, BehaviorAnimationKeyDelivery, BehaviorAnimationTextKeys,
            BehaviorInstructionId, BehaviorResumeOutcome, BehaviorStepFixedSet,
            DispatchBehaviorAnimationTextKeys, ResumeSuspendedBehaviorInstruction,
            RunBehaviorAnimationEffect, RunningBehaviorInstruction, SuspendedBehaviorSetInterrupt,
            TimedBehaviorAnimationEffect,
        },
        BehaviorTaskExecutionState, PendingBehaviorAnimationClipCompletion,
        PendingBehaviorTaskFailure,
    },
    guests::guest_simulation_types::GuestRng,
    placement::{
        placed_object_types::PlacedObjectDefinitionReference,
        placement_transaction_types::DestroyPlacedObject,
    },
    topology::topology_graph_types::TopologyGrid,
};

use super::behavior_target_morph_execution::{resolve_fence_morph, FenceTopology};
use crate::animal_health::disease_mortality_execution::finalize_animal_death;

#[derive(Component)]
pub(super) struct CrossedBehaviorAnimationEffect {
    step: BehaviorInstructionId,
    angles: Option<(f32, f32)>,
}

impl CrossedBehaviorAnimationEffect {
    pub(super) fn detach(instruction: &RunningBehaviorInstruction) -> Self {
        Self {
            step: instruction.id(),
            angles: None,
        }
    }

    pub(super) fn belongs_to(&self, instruction: &RunningBehaviorInstruction) -> bool {
        self.step == instruction.id()
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct BehaviorAnimationDetachReady;

pub(super) fn register(app: &mut App) {
    app.add_message::<DispatchBehaviorAnimationTextKeys>()
        .add_observer(discard_crossed_animation_keys)
        .add_systems(
            FixedUpdate,
            (
                (
                    dispatch_animation_key_handlers.in_set(BehaviorAnimationKeyDelivery),
                    arm_timed_animation_effects.after(BehaviorAnimationKeyDelivery).in_set(BehaviorAnimationDetachReady),
                )
                    .in_set(AnimationObjectCommandDelivery)
                    .before(super::behavior_animation_clip_execution::finish_behavior_animation_clip_actions_after_playback_completion),
                interrupt_sets_after_target_collision,
                remove_faded_animation_key_targets,
            )
                .in_set(FixedGameSet::Act)
                .before(BehaviorStepFixedSet::Act)
                .run_if(in_state(openzt2_game_assets::application_lifecycle::GamePhase::InGame)),
        )
        .add_observer(activated_behavior_actor::<RunBehaviorAnimationEffect>.pipe(apply_behavior_animation_effects))
        .add_systems(FixedUpdate, update_waiting_behavior_actors.pipe(apply_behavior_animation_effects)
            .in_set(FixedGameSet::Act)
            .after(arm_timed_animation_effects)
            .before(super::behavior_animation_clip_execution::finish_behavior_animation_clip_actions_after_playback_completion));
}

fn discard_crossed_animation_keys(
    removed: On<Remove<openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorAnimationClip>>,
    mut commands: Commands,
) {
    commands
        .entity(removed.entity)
        .try_remove::<CrossedBehaviorAnimationEffect>();
}

type AnimationKeyActor = (
    &'static BehaviorTaskExecutionState,
    &'static RunningBehaviorInstruction,
    &'static PendingBehaviorAnimationClipCompletion,
);

fn dispatch_animation_key_handlers(
    mut input: MessageReader<AnimationAiCommandRequested>,
    mut effects: MessageWriter<DispatchBehaviorAnimationTextKeys>,
    mut object_commands: MessageWriter<AnimationObjectCommand>,
    controllers: Query<(
        &AnimationPresentationOwner,
        &AnimationPlaybackController,
        Option<&AnimationRigJoints>,
    )>,
    joints: Query<&AnimationJointTarget>,
    tasks: Query<
        (
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            Option<&PendingBehaviorAnimationClipCompletion>,
            Option<&BehaviorAnimationTextKeys>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    attachment_keys: super::behavior_object_attachment_execution::AnimationAttachmentKeyOwners,
    mut interruptions: BehaviorSetInterruptions,
    mut commands: Commands,
) {
    for key in input.read() {
        let Ok((owner, playback, rig)) = controllers.get(key.controller) else {
            continue;
        };
        let actor = owner.gameplay_entity;
        let Ok((task, step, pending, keys)) = tasks.get(actor) else {
            continue;
        };
        let Some(request_id) = playback.explicit_clip_request_id else {
            continue;
        };
        if key.playback_generation != playback.playback_generation
            || key.request_id != Some(request_id)
            || (!pending.is_some_and(|pending| {
                pending.request_id == request_id && pending.owns(task, step)
            }) && !attachment_keys.owns_request(actor, task, step, request_id))
        {
            continue;
        }
        let trigger = AssetId::from_key(&key.command.trim().to_ascii_lowercase());
        if interruptions.interrupt(actor, task, trigger, task.target(), &mut commands) {
            continue;
        }
        let Some((_, handlers)) =
            keys.and_then(|keys| keys.0.iter().find(|(id, _)| *id == trigger))
        else {
            continue;
        };
        let mut presentation = Vec::new();
        for handler in handlers {
            if matches!(
                handler.animation_text_action,
                AuthoredAnimationTextAction::AttachPendingObject
                    | AuthoredAnimationTextAction::AttachNamedObject { .. }
                    | AuthoredAnimationTextAction::DetachObject
                    | AuthoredAnimationTextAction::SpawnObject
                    | AuthoredAnimationTextAction::KillAnimationSubject
            ) {
                let joint = handler
                    .target_skeleton_joint_asset_key
                    .as_ref()
                    .and_then(|joint| {
                        let id = AssetId::from_key(joint);
                        rig?.joints().iter().copied().find(|joint| {
                            joints
                                .get(*joint)
                                .is_ok_and(|joint| joint.authored_joint_id == id)
                        })
                    })
                    .unwrap_or(key.target);
                object_commands.write(AnimationObjectCommand {
                    controller: key.controller,
                    playback_generation: playback.playback_generation,
                    request_id: Some(request_id),
                    joint,
                    action: handler.animation_text_action.clone(),
                });
            } else {
                presentation.push(handler.clone());
            }
        }
        if !presentation.is_empty() {
            effects.write(DispatchBehaviorAnimationTextKeys {
                controller: key.controller,
                playback_generation: key.playback_generation,
                request_id,
                joint: key.target,
                milliseconds: key.milliseconds,
                commands: presentation,
            });
        }
    }
}

fn arm_timed_animation_effects(
    mut commands: Commands,
    mut whaps: MessageReader<AnimationWhapRequested>,
    mut keys: MessageReader<AnimationObjectCommand>,
    controllers: Query<(&AnimationPresentationOwner, &AnimationPlaybackController)>,
    tasks: Query<
        (AnimationKeyActor, &TimedBehaviorAnimationEffect),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    for key in keys.read() {
        let Ok((owner, playback)) = controllers.get(key.controller) else {
            continue;
        };
        let actor = owner.gameplay_entity;
        let Ok(((task, step, pending), effect)) = tasks.get(actor) else {
            continue;
        };
        let matches_key = matches!(
            (&key.action, effect.0),
            (
                AuthoredAnimationTextAction::KillAnimationSubject,
                BehaviorAnimationEffect::Kill { .. }
            ) | (
                AuthoredAnimationTextAction::DetachObject,
                BehaviorAnimationEffect::Detach { .. }
            )
        );
        if matches_key
            && key.playback_generation == playback.playback_generation
            && key.request_id == Some(pending.request_id)
            && playback.explicit_clip_request_id == key.request_id
            && pending.owns(task, step)
        {
            commands
                .entity(actor)
                .insert(CrossedBehaviorAnimationEffect {
                    step: step.id(),
                    angles: None,
                });
        }
    }
    for key in whaps.read() {
        let Ok((owner, playback)) = controllers.get(key.controller) else {
            continue;
        };
        let actor = owner.gameplay_entity;
        let Ok(((task, step, pending), effect)) = tasks.get(actor) else {
            continue;
        };
        if matches!(effect.0, BehaviorAnimationEffect::Whap { .. })
            && key.playback_generation == playback.playback_generation
            && key.request_id == Some(pending.request_id)
            && playback.explicit_clip_request_id == key.request_id
            && pending.owns(task, step)
        {
            commands
                .entity(actor)
                .insert(CrossedBehaviorAnimationEffect {
                    step: step.id(),
                    angles: Some((key.angle_degrees as f32, key.random_offset_degrees as f32)),
                });
        }
    }
}

#[derive(SystemParam)]
struct BehaviorSetInterruptions<'w, 's> {
    continuations: BehaviorContinuationFrames<'w, 's>,
    rules: Query<'w, 's, &'static SuspendedBehaviorSetInterrupt>,
    clock: Single<'w, 's, &'static ZooClock>,
    clips: Query<'w, 's, &'static PendingBehaviorAnimationClipCompletion>,
    cancellations: MessageWriter<'w, CancelAnimationClipPlaybackRequest>,
}

impl BehaviorSetInterruptions<'_, '_> {
    fn interrupt(
        &mut self,
        actor: Entity,
        task: &BehaviorTaskExecutionState,
        event: AssetId,
        target: Option<Entity>,
        commands: &mut Commands,
    ) -> bool {
        let depth = self
            .continuations
            .frame_entities(actor)
            .enumerate()
            .filter_map(|(depth, entity)| self.rules.get(entity).ok().map(|rule| (depth, rule)))
            .filter(|(_, rule)| {
                rule.trigger == event
                    && (event != AssetId::from_key("collidewithtarget")
                        || (target.is_some() && rule.target == target))
            })
            .last()
            .map(|(depth, _)| depth);
        let Some(depth) = depth else {
            return false;
        };
        let Some(resume) = self.continuations.frames(actor).nth(depth).cloned() else {
            return false;
        };
        let mut origin = resume.clone();
        origin.action = origin.action.saturating_sub(1);
        if let Ok(clip) = self.clips.get(actor) {
            self.cancellations
                .write(CancelAnimationClipPlaybackRequest {
                    animation_subject_entity: actor,
                    request_id: clip.request_id,
                });
            commands
                .entity(actor)
                .try_remove::<PendingBehaviorAnimationClipCompletion>();
        }
        commands.trigger(ResumeSuspendedBehaviorInstruction {
            actor,
            execution_id: task.execution_id,
            origin: resume.clone(),
            depth,
            instruction: origin,
            tick: self.clock.tick,
            outcome: BehaviorResumeOutcome::Advance,
        });
        true
    }
}

fn interrupt_sets_after_target_collision(
    mut collisions: MessageReader<CollisionStart>,
    parents: Query<&ChildOf>,
    actors: Query<&BehaviorTaskExecutionState, Without<PendingBehaviorTaskFailure>>,
    mut interruptions: BehaviorSetInterruptions,
    mut commands: Commands,
) {
    let collision = AssetId::from_key("collidewithtarget");
    for contact in collisions.read() {
        let sides = [
            (contact.collider1, contact.body1),
            (contact.collider2, contact.body2),
        ];
        for [(collider, body), (other, other_body)] in [sides, [sides[1], sides[0]]] {
            let lineage =
                |entity| std::iter::once(entity).chain(parents.iter_ancestors::<ChildOf>(entity));
            let Some((actor, task)) = body
                .into_iter()
                .chain(lineage(collider))
                .find_map(|actor| actors.get(actor).ok().map(|task| (actor, task)))
            else {
                continue;
            };
            let Some(target) = task.target().filter(|target| {
                other_body == Some(*target) || lineage(other).any(|entity| entity == *target)
            }) else {
                continue;
            };
            interruptions.interrupt(actor, task, collision, Some(target), &mut commands);
        }
    }
}

#[derive(SystemParam)]
struct AnimalAnimationEffectTargets<'w, 's> {
    entities: Query<
        'w,
        's,
        (
            Entity,
            &'static WorldMember,
            &'static DefinitionId,
            &'static GlobalTransform,
            Has<Animal>,
            Has<Dead>,
            Has<PlacedObjectDefinitionReference>,
        ),
    >,
    physical: Query<'w, 's, (&'static RigidBody, &'static mut LinearVelocity)>,
    poses: Query<'w, 's, &'static mut Transform>,
    clips: MessageWriter<'w, AnimationClipPlaybackRequest>,
    destroy: MessageWriter<'w, DestroyPlacedObject>,
    deaths: MessageWriter<'w, AnimalDied>,
}

fn apply_behavior_animation_effects(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    clock: Single<&ZooClock>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    grid: Single<&TopologyGrid>,
    mut topology: FenceTopology,
    mut targets: AnimalAnimationEffectTargets,
    mut actors: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            Option<&RunBehaviorAnimationEffect>,
            Option<&TimedBehaviorAnimationEffect>,
            Option<&CrossedBehaviorAnimationEffect>,
            Option<&mut BehaviorRandomStream>,
            Option<&mut GuestRng>,
        ),
        (
            Without<PendingBehaviorTaskFailure>,
            Or<(
                With<RunBehaviorAnimationEffect>,
                With<TimedBehaviorAnimationEffect>,
            )>,
        ),
    >,
) {
    let Some(view) = definitions.get(&assets) else {
        return;
    };
    let mut killed = HashSet::<Entity>::default();
    for (actor, task, step, immediate, timed, crossed, mut animal_random, mut guest_random) in
        behavior_action_actors(&mut actors, activation.0)
    {
        if !step.is_current(task) || killed.contains(&actor) {
            continue;
        }
        let (effect, angles) = if let Some(immediate) =
            immediate.filter(|_| activation.0.is_some() && task.next_action_tick <= clock.tick)
        {
            (immediate.0, None)
        } else if let Some((timed, crossed)) =
            timed.zip(crossed.filter(|crossed| crossed.step == step.id()))
        {
            (timed.0, crossed.angles)
        } else {
            continue;
        };
        let Ok((_, member, _, pose, ..)) = targets.entities.get(actor) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let member = *member;
        let pose = *pose;
        match effect {
            BehaviorAnimationEffect::Detach { .. } => continue,
            BehaviorAnimationEffect::Kill {
                termination,
                radius,
                destroy,
            } => {
                let mut victims = Vec::new();
                victims.extend(termination.kill_target.then(|| task.target()).flatten());
                victims.extend(termination.kill_subject.then_some(actor));
                if let Some((radius, kind)) = radius {
                    let centre = pose.translation();
                    let extent = Vec3::splat(radius);
                    if kind == AssetId::from_key("fence") {
                        victims.extend(topology.edges_in_box(
                            **grid,
                            centre - extent,
                            centre + extent,
                        ));
                    }
                    victims.extend(targets.entities.iter().filter_map(
                        |(entity, world, definition, position, ..)| {
                            (world.root == member.root
                                && position.translation().cmpge(centre - extent).all()
                                && position.translation().cmple(centre + extent).all()
                                && (definition.0 == kind
                                    || view.find_object(definition.0).is_some_and(|object| {
                                        object.behavior_type_identifiers.contains(&kind)
                                    })))
                            .then_some(entity)
                        },
                    ));
                }
                let mut removed = false;
                for victim in victims {
                    if killed.contains(&victim) {
                        continue;
                    }
                    if let Ok((edge, ..)) = topology.fences.get(victim) {
                        let replacement = resolve_fence_morph(
                            view,
                            edge.definition,
                            AssetId::from_key("s_broken"),
                        );
                        if replacement.is_some_and(|replacement| {
                            topology.replace(victim, replacement, false, &mut commands)
                        }) {
                            killed.insert(victim);
                            removed = true;
                        }
                    } else if targets.kill(
                        victim,
                        member.root,
                        task.program,
                        clock.tick,
                        termination.fade_out_period,
                        destroy,
                        view,
                        &mut commands,
                    ) {
                        killed.insert(victim);
                        removed = true;
                    }
                }
                if !removed && (termination.kill_subject || termination.kill_target) {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                }
            }
            BehaviorAnimationEffect::Whap {
                angle_degrees,
                random_offset_degrees,
            } => {
                let Some(target) = task.target().filter(|target| {
                    targets
                        .entities
                        .get(*target)
                        .is_ok_and(|(_, world, ..)| world.root == member.root)
                }) else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                let Some(random) = animal_random
                    .as_deref_mut()
                    .map(|random| &mut random.0)
                    .or_else(|| guest_random.as_deref_mut().map(|random| &mut random.0))
                else {
                    continue;
                };
                let (key_angle, key_offset) = angles.unwrap_or_default();
                let heading = angle_degrees.unwrap_or(key_angle)
                    + (2.0 * random.unit_f32() - 1.0) * random_offset_degrees.unwrap_or(key_offset);
                let direction =
                    (pose.rotation() * Quat::from_rotation_y(heading.to_radians()) * Vec3::Z)
                        .xz()
                        .normalize_or_zero();
                targets.whap(target, direction, view, random, &mut commands);
            }
        }
        if !killed.contains(&actor) {
            commands.entity(actor).try_remove::<(
                CrossedBehaviorAnimationEffect,
                PendingBehaviorAnimationClipCompletion,
            )>();
            step.complete(actor, clock.tick, &mut commands);
        }
    }
}

impl AnimalAnimationEffectTargets<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    fn kill(
        &mut self,
        victim: Entity,
        world: Entity,
        program: AssetId,
        tick: u64,
        fade: std::time::Duration,
        destroy: bool,
        view: WorldDefinitionsView<'_>,
        commands: &mut Commands,
    ) -> bool {
        let Ok((_, membership, _, _, animal, dead, placed)) = self.entities.get(victim) else {
            return false;
        };
        if membership.root != world || dead {
            return false;
        }
        if animal {
            finalize_animal_death(
                commands,
                &mut self.deaths,
                victim,
                DeathCause::Authored(program),
                tick,
            );
            let ticks = fade
                .as_nanos()
                .saturating_mul(u128::from(view.timing().fixed_hz))
                .div_ceil(1_000_000_000);
            commands.entity(victim).insert(
                super::behavior_termination_execution::AuthoredAnimalRemovalTick(
                    tick.saturating_add(u64::try_from(ticks).unwrap_or(u64::MAX)),
                ),
            );
        } else if destroy && placed {
            self.destroy.write(DestroyPlacedObject { entity: victim });
        } else {
            fade_and_remove_authored_object(commands, victim, tick, view.timing().fixed_hz, fade);
        }
        true
    }

    fn whap(
        &mut self,
        target: Entity,
        heading: Vec2,
        view: WorldDefinitionsView<'_>,
        random: &mut openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng,
        commands: &mut Commands,
    ) {
        let Ok((_, _, definition, ..)) = self.entities.get(target) else {
            return;
        };
        let Some(physics) = view
            .find_object(definition.0)
            .and_then(|object| object.authored_runtime.whap_physics.as_ref())
        else {
            return;
        };
        match physics {
            WorldObjectWhapPhysics::Real {
                speed,
                lob_pitch_degrees,
                fit_state,
            } => {
                let Ok((body, mut velocity)) = self.physical.get_mut(target) else {
                    return;
                };
                let mut draw = |[low, high]: [f32; 2]| (high - low).mul_add(random.unit_f32(), low);
                let (horizontal, vertical) = if let Some(pitch) = lob_pitch_degrees {
                    let pitch = draw(*pitch).to_radians();
                    let speed = draw(*speed);
                    (pitch.cos() * speed, pitch.sin() * speed)
                } else {
                    let horizontal = draw(*speed);
                    let vertical = if *fit_state == 4 {
                        2.0 * draw(*speed)
                    } else {
                        draw([0.0, horizontal])
                    };
                    (horizontal, vertical)
                };
                if *body != RigidBody::Dynamic {
                    commands.entity(target).insert(RigidBody::Dynamic);
                }
                velocity.set_if_neq(LinearVelocity(Vec3::new(
                    heading.x * horizontal,
                    vertical,
                    heading.y * horizontal,
                )));
                commands.entity(target).try_remove::<Sleeping>();
            }
            WorldObjectWhapPhysics::Animated {
                motion_clip,
                rotate_to_heading,
            } => {
                if *rotate_to_heading {
                    if let Ok(mut pose) = self.poses.get_mut(target) {
                        pose.rotation = Quat::from_rotation_y(heading.x.atan2(heading.y));
                    }
                }
                self.clips.write(AnimationClipPlaybackRequest {
                    animation_subject_entity: target,
                    animation_clip_asset_key: motion_clip.clone(),
                    blend_duration_milliseconds: 0,
                    playback_speed_permille: 1000,
                    playback_repetition_policy: AnimationPlaybackRepetitionPolicy::PlayOnce,
                });
            }
        }
    }
}

pub(super) fn fade_and_remove_authored_object(
    commands: &mut Commands,
    victim: Entity,
    tick: u64,
    fixed_hz: u16,
    fade: std::time::Duration,
) {
    if fade.is_zero() {
        commands.entity(victim).try_despawn();
    } else {
        let delay = fade
            .as_nanos()
            .saturating_mul(u128::from(fixed_hz))
            .div_ceil(1_000_000_000);
        commands.entity(victim).insert((
            openzt2_world::world_spawn::prefab_world_instance_spawning::morph_fade_projection::PrefabMorphFade::fade_out(fade),
            PendingAnimationKeyRemoval(tick.saturating_add(u64::try_from(delay).unwrap_or(u64::MAX))),
        ));
    }
}

#[derive(Component)]
struct PendingAnimationKeyRemoval(u64);

fn remove_faded_animation_key_targets(
    clock: Single<&ZooClock>,
    victims: Query<(Entity, &PendingAnimationKeyRemoval)>,
    mut commands: Commands,
) {
    for (victim, due) in &victims {
        if clock.tick >= due.0 {
            commands.entity(victim).try_despawn();
        }
    }
}
