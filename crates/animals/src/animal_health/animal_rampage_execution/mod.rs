use bevy::prelude::*;
use std::time::Duration;

use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_game_data::species::SpeciesVariantFlags;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_world_state::world_spawn::world_entity_crating::UncratedWorldEntity;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::animal_behavior::behavior_random_stream_state::BehaviorRandomStream;
use openzt2_zoo_state::animal_lifecycle::types::{Animal, AnimalVariant};
use openzt2_zoo_state::animal_welfare::types::NeedPoints;
use openzt2_zoo_state::audio::audio_playback_message_types::{PlayAudioCue, StopAudioCue};

use super::rampage_policy::{RampagePolicy, RampagePolicySource};
use super::types::{Dead, Rampaging, Tranquilized};

#[derive(Component)]
struct RampageStartCheck(Timer);

#[derive(Component)]
struct RampageTimeRemaining(Timer);

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RampageAudioMixProjection;

#[derive(Component, PartialEq)]
pub struct RampageAudioMix {
    pub loop_cue: openzt2_game_data::AssetId,
    pub loop_selection: u64,
    pub loop_gain: f32,
    pub other_sound_gain: f32,
}

fn rampage_duration(seconds: f32) -> Duration {
    Duration::try_from_secs_f32(seconds).unwrap_or_default()
}

#[derive(bevy::ecs::system::SystemParam)]
struct ActiveRampagePolicy<'w> {
    source: Res<'w, RampagePolicySource>,
    assets: Res<'w, Assets<RampagePolicy>>,
}

impl ActiveRampagePolicy<'_> {
    fn get(&self) -> Option<&RampagePolicy> {
        self.assets.get(&self.source.0)
    }
}

pub(super) fn register_rampage_timers(app: &mut App) {
    use openzt2_world_state::application_schedule::FixedGameSet;

    app.add_systems(
        FixedUpdate,
        (
            project_rampage_eligibility,
            advance_rampage_cooldowns,
            check_due_animals_for_rampage
                .after(project_rampage_eligibility)
                .after(advance_rampage_cooldowns),
            end_expired_rampages.after_ignore_deferred(check_due_animals_for_rampage),
        )
            .in_set(FixedGameSet::Think)
            .run_if(super::in_state(super::GamePhase::InGame)),
    )
    .add_systems(
        Update,
        project_rampage_music
            .in_set(RampageAudioMixProjection)
            .in_set(openzt2_world_state::application_schedule::GameSet::Presentation)
            .run_if(super::in_state(super::GamePhase::InGame)),
    )
    .add_observer(stop_music_when_zoo_closes);
}

fn project_rampage_music(
    policy: ActiveRampagePolicy,
    time: Res<Time<Real>>,
    active: Query<(&WorldMember, &GlobalTransform), (With<Rampaging>, Without<Dead>)>,
    cameras: Query<(&WorldMember, &Camera, &GlobalTransform), With<ZooCamera>>,
    mut worlds: Query<(Entity, Option<&mut RampageAudioMix>), With<WorldRoot>>,
    mut commands: Commands,
    mut play: MessageWriter<PlayAudioCue>,
    mut stop: MessageWriter<StopAudioCue>,
) {
    let Some(policy) = policy.get() else {
        return;
    };
    let listeners = cameras
        .iter()
        .filter(|(_, camera, _)| camera.is_active)
        .map(|(member, _, pose)| (member.root, pose.translation()))
        .collect::<bevy::ecs::entity::EntityHashMap<_>>();
    let mut distances = bevy::ecs::entity::EntityHashMap::<f32>::default();
    for (member, pose) in &active {
        let distance = listeners
            .get(&member.root)
            .map_or(f32::INFINITY, |listener| {
                pose.translation().distance_squared(*listener)
            });
        let nearest = distances.entry(member.root).or_insert(f32::INFINITY);
        *nearest = nearest.min(distance);
    }
    for (root, playing) in &mut worlds {
        let selection = root.to_bits();
        if let Some(&nearest) = distances.get(&root) {
            let same_cue = playing
                .as_ref()
                .is_some_and(|playing| playing.loop_cue == policy.music);
            if !same_cue {
                if playing.is_some() {
                    stop.write(StopAudioCue {
                        emitter: None,
                        selection,
                    });
                }
                if policy.music != openzt2_game_data::AssetId::default() {
                    play.write(PlayAudioCue {
                        cue: policy.music,
                        emitter: None,
                        selection,
                        priority: 192,
                        force_looped: true,
                    });
                }
            }
            let distance = (nearest.sqrt() - policy.full_gain_distance_m).max(0.0);
            let target = if distance < policy.distance_span_m {
                (1.0 - distance / policy.distance_span_m).powi(2)
            } else {
                0.0
            };
            let loop_gain = if policy.fade_seconds > 0.0 && same_cue {
                let current = playing.as_ref().map_or(0.0, |playing| playing.loop_gain);
                let step = time.delta_secs() / policy.fade_seconds;
                current + (target - current).clamp(-step, step)
            } else if policy.fade_seconds == 0.0 {
                target
            } else {
                0.0
            };
            let mix = RampageAudioMix {
                loop_cue: policy.music,
                loop_selection: selection,
                loop_gain,
                other_sound_gain: policy.ducking_gain,
            };
            if let Some(mut playing) = playing {
                playing.set_if_neq(mix);
            } else {
                commands.entity(root).insert(mix);
            }
        } else if playing.is_some() {
            stop.write(StopAudioCue {
                emitter: None,
                selection,
            });
            commands.entity(root).remove::<RampageAudioMix>();
            if policy.end_music != openzt2_game_data::AssetId::default() {
                play.write(PlayAudioCue {
                    cue: policy.end_music,
                    emitter: None,
                    selection: !selection,
                    priority: 192,
                    force_looped: false,
                });
            }
        }
    }
}

fn stop_music_when_zoo_closes(
    removed: On<Remove<WorldRoot>>,
    playing: Query<(), With<RampageAudioMix>>,
    mut stop: MessageWriter<StopAudioCue>,
) {
    if playing.contains(removed.entity) {
        stop.write(StopAudioCue {
            emitter: None,
            selection: removed.entity.to_bits(),
        });
    }
}

fn project_rampage_eligibility(
    policy: ActiveRampagePolicy,
    (assets, index): (Res<Assets<SpeciesAsset>>, Res<SpeciesAssets>),
    animals: Query<
        (Entity, &AnimalVariant, Has<RampageStartCheck>),
        (With<Animal>, With<NeedPoints>, Without<Dead>),
    >,
    changed: Query<
        (Entity, &AnimalVariant, Has<RampageStartCheck>),
        (
            With<Animal>,
            With<NeedPoints>,
            Without<Dead>,
            Or<(Changed<AnimalVariant>, Added<NeedPoints>)>,
        ),
    >,
    mut policy_events: MessageReader<AssetEvent<RampagePolicy>>,
    mut commands: Commands,
) {
    let policy_changed = policy.source.is_changed()
        || policy_events.read().any(|event| {
            event.is_added(policy.source.0.id()) || event.is_modified(policy.source.0.id())
        });
    policy_events.clear();
    let (Some(policy), Some(species)) = (policy.get(), index.get(&assets)) else {
        return;
    };
    let definitions_changed = policy_changed || assets.is_changed() || index.is_changed();
    let affected = definitions_changed
        .then(|| animals.iter())
        .into_iter()
        .flatten()
        .chain(
            (!definitions_changed)
                .then(|| changed.iter())
                .into_iter()
                .flatten(),
        );
    for (animal, variant, scheduled) in affected {
        let eligible = species
            .find_variant(variant.0)
            .is_some_and(|variant| variant.flags.contains(SpeciesVariantFlags::RAMPAGER));
        if eligible && !scheduled {
            commands.entity(animal).insert(RampageStartCheck(Timer::new(
                rampage_duration(policy.check_interval),
                TimerMode::Repeating,
            )));
        } else if scheduled && !eligible {
            commands
                .entity(animal)
                .remove::<(RampageStartCheck, RampageTimeRemaining, Rampaging)>();
        }
    }
}

fn advance_rampage_cooldowns(
    time: Res<Time<Fixed>>,
    mut cooldowns: Query<
        (Entity, &mut RampageTimeRemaining),
        (Without<Rampaging>, Without<Tranquilized>, Without<Dead>),
    >,
    mut commands: Commands,
) {
    for (animal, mut cooldown) in &mut cooldowns {
        if cooldown.0.tick(time.timestep()).is_finished() {
            commands.entity(animal).remove::<RampageTimeRemaining>();
        }
    }
}

fn check_due_animals_for_rampage(
    policy: ActiveRampagePolicy,
    time: Res<Time<Fixed>>,
    rampagers: Query<(), (With<Animal>, With<Rampaging>, Without<Dead>)>,
    mut checks: Query<(Entity, &mut RampageStartCheck), (Without<Rampaging>, Without<Dead>)>,
    mut eligible: Query<
        (&NeedPoints, &mut BehaviorRandomStream),
        (
            Without<Tranquilized>,
            UncratedWorldEntity,
            Without<RampageTimeRemaining>,
        ),
    >,
    mut commands: Commands,
) {
    let Some(policy) = policy.get() else {
        return;
    };
    let mut rampaging_now = None;
    for (animal, mut check) in &mut checks {
        if check.is_added() {
            continue;
        }
        if !check.0.tick(time.timestep()).just_finished() {
            continue;
        }
        check
            .0
            .set_duration(rampage_duration(policy.check_interval));
        let Ok((points, mut random)) = eligible.get_mut(animal) else {
            continue;
        };
        let chance = rampage_chance_percent(
            policy,
            points,
            *rampaging_now.get_or_insert_with(|| rampagers.iter().count()),
        );
        if (random.next_u32() % 10_000) as f32 >= chance * 100.0 {
            continue;
        }
        let variance = (random.next_u32() % 10_001) as f32 / 10_000.0 * 2.0 - 1.0;
        commands.entity(animal).insert((
            Rampaging,
            RampageTimeRemaining(Timer::new(
                rampage_duration(policy.duration + policy.variance * variance),
                TimerMode::Once,
            )),
        ));
    }
}

fn rampage_chance_percent(
    policy: &RampagePolicy,
    points: &NeedPoints,
    rampaging_now: usize,
) -> f32 {
    if points.good > policy.good_point_limit {
        return 0.0;
    }
    let chance = if points.bad > policy.bad_point_limit {
        policy.maximum_chance
    } else if points.bad > 0.0 {
        policy.base_chance
            + (policy.maximum_chance - policy.base_chance) * points.bad / policy.bad_point_limit
    } else {
        policy.base_chance
    };
    chance * policy.population_factor.powi(rampaging_now.min(64) as i32)
}

fn end_expired_rampages(
    policy: ActiveRampagePolicy,
    time: Res<Time<Fixed>>,
    mut active: Query<
        (Entity, Option<&mut RampageTimeRemaining>),
        (
            With<Rampaging>,
            With<RampageStartCheck>,
            Without<Tranquilized>,
            Without<Dead>,
        ),
    >,
    mut commands: Commands,
) {
    let Some(policy) = policy.get() else {
        return;
    };
    for (animal, timer) in &mut active {
        if let Some(mut timer) = timer {
            if !timer.0.tick(time.timestep()).is_finished() {
                continue;
            }
            timer.0.set_duration(rampage_duration(policy.cooldown));
            timer.0.reset();
            commands.entity(animal).remove::<Rampaging>();
        } else {
            commands
                .entity(animal)
                .remove::<Rampaging>()
                .insert(RampageTimeRemaining(Timer::new(
                    rampage_duration(policy.cooldown),
                    TimerMode::Once,
                )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points(good: f32, bad: f32) -> NeedPoints {
        NeedPoints { good, bad }
    }

    #[test]
    fn rampage_chance_follows_the_original_manager() {
        let close = |actual: f32, expected: f32| (actual - expected).abs() < 1e-4;
        let policy = super::super::rampage_policy::parse_rampage_policy(br#"<BFGRampageMgr updateInterval="3.5" maxGoodPoints="10" maxBadPoints="60" baseRampageChance="0.2" maxRampageChance="4" rampagingAnimalFactor="1.2" rampageTimeout="300" rampageVariance="30" rampageInterval="300"/>"#).unwrap();
        // No bad points leave the base chance; too many good ones rule it out.
        assert!(close(
            rampage_chance_percent(&policy, &points(0.0, 0.0), 0),
            0.2
        ));
        assert_eq!(rampage_chance_percent(&policy, &points(10.5, 60.0), 0), 0.0);
        assert!(close(
            rampage_chance_percent(&policy, &points(10.0, 30.0), 0),
            2.1
        ));
        assert!(close(
            rampage_chance_percent(&policy, &points(0.0, 60.0), 0),
            4.0
        ));
        assert!(close(
            rampage_chance_percent(&policy, &points(0.0, 90.0), 0),
            4.0
        ));
        // Two animals already rampaging: 2.1 x 1.2 x 1.2.
        assert!(close(
            rampage_chance_percent(&policy, &points(0.0, 30.0), 2),
            3.024
        ));
    }
}
