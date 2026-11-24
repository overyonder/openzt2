use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::{
    effect::SpawnCompleteParticleEffectDocument,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{
    AssetId,
    animation::animation_text_key::{
        AuthoredAnimationSurfaceEffectKind, AuthoredAnimationTextAction,
    },
    behavior::action::token::BehaviorTokenRecipient,
};
use openzt2_world_state::{
    animation_graph::{
        animation_graph_playback_message_types::{
            AnimationClipActionOptions, AnimationClipPlaybackRequest,
            AnimationClipPlaybackRequestRejected,
        },
        animation_presentation_relationship_types::AnimationPresentationOwner,
    },
    animation_playback::{
        animation_event_message_types::{
            AnimationAiCommandRequested, AnimationCompleted, AnimationObjectCommand,
            AnimationObjectCommandDelivery,
        },
        animation_playback_controller_types::AnimationPlaybackRepetitionPolicy,
    },
    application_schedule::FixedGameSet,
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    animal_behavior::behavior_random_stream_state::BehaviorRandomStream,
    animal_behavior::behavior_token_types::BehaviorSelfToken,
    aquatic::aquatic_simulation_types::TankGeometry,
    audio::audio_playback_message_types::PlayPositionedAudioCue,
    behavior_task_execution_types::{
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
        running_actions::{
            RestoredSplashAnimation, RunBehaviorSplashGuest, RunningBehaviorInstruction,
            SplashAnimation, SplashAnimationSeek, WhenBehaviorAction,
        },
    },
    guests::guest_simulation_types::Guest,
    habitat::habitat_types::HabitatMember,
    shows::{running_show_types::ShowViewingLocationOf, show_event_types::AuthoredAnimalShowEvent},
};

use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;

#[derive(SystemParam)]
struct SplashEffects<'w, 's> {
    definitions: Res<'w, WorldDefinitions>,
    server: Res<'w, AssetServer>,
    assets: Res<'w, Assets<WorldDefinitionAsset>>,
    viewers: Query<
        'w,
        's,
        (
            Entity,
            &'static WorldMember,
            &'static GlobalTransform,
            Has<ShowViewingLocationOf>,
        ),
        Or<(With<Guest>, With<ShowViewingLocationOf>)>,
    >,
    source: Query<
        'w,
        's,
        (
            &'static GlobalTransform,
            &'static WorldMember,
            Option<&'static HabitatMember>,
            Option<&'static mut BehaviorRandomStream>,
        ),
    >,
    water: Query<'w, 's, &'static TankGeometry>,
    particles: MessageWriter<'w, SpawnCompleteParticleEffectDocument>,
    audio: MessageWriter<'w, PlayPositionedAudioCue>,
}

pub(super) fn register(app: &mut App) {
    app.add_observer(start_splash)
        .add_observer(clear_splash_animation)
        .add_systems(
            FixedUpdate,
            (
                emit_splashes_at_authored_keys.in_set(AnimationObjectCommandDelivery),
                finish_splash_animations.after(emit_splashes_at_authored_keys),
            )
                .in_set(FixedGameSet::Act),
        );
}

fn start_splash(
    activation: On<WhenBehaviorAction<RunBehaviorSplashGuest>>,
    clock: Single<&ZooClock>,
    scenes: Query<(
        &RunBehaviorSplashGuest,
        &RunningBehaviorInstruction,
        Option<&RestoredSplashAnimation>,
    )>,
    mut playback: MessageWriter<AnimationClipPlaybackRequest>,
    mut options: MessageWriter<AnimationClipActionOptions>,
    mut effects: SplashEffects,
    mut commands: Commands,
) {
    let actor = activation.entity;
    let Ok((action, step, restored)) = scenes.get(actor) else {
        return;
    };
    if let Some(animation) = &action.0.animation {
        let request = playback
            .write(AnimationClipPlaybackRequest {
                animation_subject_entity: actor,
                animation_clip_asset_key: animation.clone(),
                blend_duration_milliseconds: 0,
                playback_speed_permille: 1000,
                playback_repetition_policy: AnimationPlaybackRepetitionPolicy::PlayOnce,
            })
            .id;
        options.write(AnimationClipActionOptions {
            request_id: request,
            binder: None,
            ground_fit: action.0.ground_fit,
            interrupts_current_animation: action.0.interrupts_animation,
            interruptible: false,
        });
        let mut actor_commands = commands.entity(actor);
        actor_commands.insert(SplashAnimation {
            step: step.id(),
            request,
            emitted: restored.is_some_and(|restored| restored.emitted),
        });
        if let Some(restored) = restored {
            if restored.seek_seconds.is_finite() && restored.seek_seconds > 0.0 {
                actor_commands.insert(SplashAnimationSeek {
                    request,
                    seconds: restored.seek_seconds,
                });
            }
            actor_commands.remove::<RestoredSplashAnimation>();
        }
    } else if effects.emit(actor, step, action, None, None, clock.tick, &mut commands) {
        step.complete(actor, clock.tick, &mut commands);
    } else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
    }
}

impl SplashEffects<'_, '_> {
    fn emit(
        &mut self,
        actor: Entity,
        step: &RunningBehaviorInstruction,
        action: &RunBehaviorSplashGuest,
        joint: Option<&GlobalTransform>,
        named_surface_effect: Option<&str>,
        tick: u64,
        commands: &mut Commands,
    ) -> bool {
        let action = &action.0;
        let Some(view) = self.definitions.get(&self.assets) else {
            return false;
        };
        let effect = action
            .particle
            .as_deref()
            .or(named_surface_effect)
            .map(|name| {
                view.effect(AssetId::from_key(name)).unwrap_or_else(|| {
                    let name = name.to_ascii_lowercase();
                    self.server
                        .load(format!("particle/{}.psys", name.trim_end_matches(".psys")))
                })
            });
        let Ok((pose, world, habitat, mut random)) = self.source.get_mut(actor) else {
            return false;
        };
        let origin = pose.translation();
        let anchor = joint.unwrap_or(pose);
        let authored_direction = action.direction.map(Vec3::from_array).unwrap_or(Vec3::Z);
        let direction = (anchor.rotation() * authored_direction)
            .xz()
            .normalize_or_zero();
        let nearest = self
            .viewers
            .iter()
            .filter_map(|(viewer, location, position, stage)| {
                if location.root != world.root {
                    return None;
                }
                let delta = (position.translation() - origin).xz();
                let distance = delta.length();
                if distance > 20.0
                    || (distance > f32::EPSILON && delta.dot(direction) < distance * 0.866_025_4)
                {
                    return None;
                }
                Some((!stage, distance, viewer, position.translation()))
            })
            .min_by(|left, right| {
                left.0
                    .cmp(&right.0)
                    .then_with(|| left.1.total_cmp(&right.1))
                    .then_with(|| left.2.cmp(&right.2))
            });
        let mut destination = if let Some((_, _, _, position)) = nearest {
            position
        } else if let Some(random) = random.as_deref_mut() {
            let distance = 21.0_f32.mul_add(random.0.unit_f32(), -1.0);
            let azimuth = 2.0_f32.mul_add(random.0.unit_f32(), -1.0) * std::f32::consts::FRAC_PI_6;
            origin
                + Quat::from_rotation_y(azimuth)
                    * Vec3::new(direction.x, 0.0, direction.y)
                    * distance
        } else {
            origin
        };
        if action.from_water_surface {
            if let Some(water) =
                habitat.and_then(|habitat| self.water.get(habitat.habitat_entity).ok())
            {
                destination.y = water.water_height;
            }
        }
        if let Some(ref effect) = effect {
            self.particles.write(SpawnCompleteParticleEffectDocument {
                particle_effect_asset: effect.clone(),
                parent_entity: None,
                inherit_parent_rotation: false,
                effect_transform: Transform::from_translation(destination)
                    .with_rotation(anchor.rotation()),
                manual_particle_count: 1,
            });
            for (cue, above_water) in [("splash_med", true), ("splash_med_uw", false)] {
                self.audio.write(PlayPositionedAudioCue {
                    cue: AssetId::from_key(cue),
                    world_position: origin,
                    selection: actor.to_bits() ^ tick,
                    priority: 128,
                    force_looped: false,
                    listener_above_water: above_water,
                    listener_below_water: !above_water,
                    admission: None,
                });
            }
        }
        if let Some(policy) = &action.show_event {
            commands.spawn((
                *world,
                AuthoredAnimalShowEvent {
                    performer: actor,
                    execution_id: step.execution_id(),
                    policy: policy.clone(),
                },
            ));
        }
        let viewer = nearest.and_then(|(guest, _, viewer, position)| {
            if guest {
                return Some(viewer);
            }
            self.viewers
                .iter()
                .filter_map(|(entity, member, guest_pose, grandstand)| {
                    (!grandstand && member.root == world.root)
                        .then_some((entity, guest_pose.translation().distance_squared(position)))
                })
                .filter(|(_, distance)| *distance <= 9.0)
                .min_by(|left, right| {
                    left.1
                        .total_cmp(&right.1)
                        .then_with(|| left.0.cmp(&right.0))
                })
                .map(|(guest, _)| guest)
        });
        let at_tick = |duration: std::time::Duration| {
            let ticks = duration
                .as_nanos()
                .saturating_mul(u128::from(view.timing().fixed_hz))
                .div_ceil(1_000_000_000);
            tick.saturating_add(u64::try_from(ticks).unwrap_or(u64::MAX))
        };
        for token in &action.tokens {
            if token.chance_percent <= 0.0
                || (token.chance_percent < 100.0
                    && random
                        .as_deref_mut()
                        .is_none_or(|random| random.0.unit_f32() * 100.0 >= token.chance_percent))
            {
                continue;
            }
            let recipient = match token.recipient {
                BehaviorTokenRecipient::Subject => Some(actor),
                BehaviorTokenRecipient::Target => viewer,
            };
            if let Some(recipient) = recipient {
                commands.queue(super::behavior_token_execution::DeliverBehaviorSelfToken {
                    actor,
                    recipient,
                    tick,
                    only_one: token.only_one,
                    reconsider: token.reconsider,
                    token: BehaviorSelfToken {
                        name: token.token,
                        available_at: at_tick(token.delay),
                        expires_at: token.lifetime.map_or(u64::MAX, at_tick),
                        payload: token.payload.and_then(|owner| match owner {
                            BehaviorTokenRecipient::Subject => Some(actor),
                            BehaviorTokenRecipient::Target => viewer,
                        }),
                        retain_on_failure: token.retain_on_failure,
                    },
                });
            }
        }
        effect.is_some() || action.show_event.is_some() || !action.tokens.is_empty()
    }
}

fn emit_splashes_at_authored_keys(
    mut keys: MessageReader<AnimationAiCommandRequested>,
    mut surface_effects: MessageReader<AnimationObjectCommand>,
    clock: Single<&ZooClock>,
    mut commands: Commands,
    presentations: Query<&AnimationPresentationOwner>,
    joints: Query<&GlobalTransform>,
    mut scenes: Query<(
        &RunBehaviorSplashGuest,
        &RunningBehaviorInstruction,
        &mut SplashAnimation,
    )>,
    mut effects: SplashEffects,
) {
    let commands_at_keys = surface_effects
        .read()
        .filter_map(|key| {
            let AuthoredAnimationTextAction::TriggerSurfaceEffect {
                surface_effect_kind: AuthoredAnimationSurfaceEffectKind::NamedSurfaceEffect,
                named_surface_effect_asset_key: Some(name),
                ..
            } = &key.action
            else {
                return None;
            };
            Some((
                key.controller,
                key.request_id,
                key.joint,
                Some(name.as_str()),
            ))
        })
        .chain(keys.read().filter_map(|key| {
            (key.command.eq_ignore_ascii_case("splash")
                || key.command.eq_ignore_ascii_case("splashguest"))
            .then_some((key.controller, key.request_id, key.target, None))
        }));
    for (controller, request, joint, effect) in commands_at_keys {
        let Ok(owner) = presentations.get(controller) else {
            continue;
        };
        let Ok((action, step, mut splash)) = scenes.get_mut(owner.gameplay_entity) else {
            continue;
        };
        if splash.step != step.id() || Some(splash.request) != request || splash.emitted {
            continue;
        }
        splash.emitted = effects.emit(
            owner.gameplay_entity,
            step,
            action,
            joints.get(joint).ok(),
            effect,
            clock.tick,
            &mut commands,
        );
    }
}

fn finish_splash_animations(
    clock: Single<&ZooClock>,
    mut completed: MessageReader<AnimationCompleted>,
    mut rejected: MessageReader<AnimationClipPlaybackRequestRejected>,
    presentations: Query<&AnimationPresentationOwner>,
    states: Query<
        (
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &SplashAnimation,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    mut commands: Commands,
) {
    for event in rejected.read() {
        if states
            .get(event.animation_subject_entity)
            .is_ok_and(|(task, step, splash)| {
                step.is_current(task)
                    && splash.step == step.id()
                    && event.request_id == splash.request
            })
        {
            mark_behavior_task_for_failure_and_stop_navigation(
                event.animation_subject_entity,
                &mut commands,
            );
        }
    }
    for event in completed.read() {
        let Ok(owner) = presentations.get(event.animation_playback_controller_entity) else {
            continue;
        };
        let actor = owner.gameplay_entity;
        let Ok((task, step, splash)) = states.get(actor) else {
            continue;
        };
        if !step.is_current(task)
            || splash.step != step.id()
            || event.explicit_clip_request_id != Some(splash.request)
        {
            continue;
        }
        if splash.emitted {
            step.complete(actor, clock.tick, &mut commands);
        } else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        }
    }
}

fn clear_splash_animation(removed: On<Remove<RunBehaviorSplashGuest>>, mut commands: Commands) {
    commands
        .entity(removed.entity)
        .try_remove::<(SplashAnimation, SplashAnimationSeek)>();
}
