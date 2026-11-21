use bevy::{
    ecs::{query::QueryData, system::SystemParam},
    platform::collections::HashMap,
    prelude::*,
};
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{species::NeedKind, AssetId};
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::simulation_time::{
    deterministic_random_stream::{DeterministicRng, RngDomain, ZooSeed},
    simulation_control_queries::simulation_is_running,
};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::{
    animal_health::types::Dead,
    animal_lifecycle::types::{Animal, SpeciesHandle},
    animal_welfare::types::*,
};

use super::{calculations::*, systems::need_rank};

#[derive(Component)]
pub(crate) struct NeedLatch<const NEED: usize>(pub(crate) bool);

#[derive(QueryData)]
pub(crate) struct AnimalNeedLatches {
    hunger: &'static NeedLatch<0>,
    thirst: &'static NeedLatch<1>,
    rest: &'static NeedLatch<2>,
    privacy: &'static NeedLatch<3>,
    social: &'static NeedLatch<4>,
    exercise: &'static NeedLatch<5>,
    stimulation: &'static NeedLatch<6>,
    environment: &'static NeedLatch<7>,
    health: &'static NeedLatch<8>,
    hygiene: &'static NeedLatch<9>,
    bathroom: &'static NeedLatch<10>,
    reproduction: &'static NeedLatch<11>,
}

impl AnimalNeedLatchesItem<'_, '_> {
    pub(crate) fn mask(&self) -> u16 {
        [
            self.hunger.0,
            self.thirst.0,
            self.rest.0,
            self.privacy.0,
            self.social.0,
            self.exercise.0,
            self.stimulation.0,
            self.environment.0,
            self.health.0,
            self.hygiene.0,
            self.bathroom.0,
            self.reproduction.0,
        ]
        .into_iter()
        .enumerate()
        .fold(0, |mask, (index, triggered)| {
            mask | (u16::from(triggered) << index)
        })
    }
}

#[derive(Message)]
pub(super) struct NeedThresholdCrossing<const NEED: usize>(pub(super) NeedChanged);

#[derive(Resource, Default)]
struct DueAnimalNeedDecay(Vec<Entity>);

#[derive(Resource, Default)]
struct AbsoluteAnimalNeedWrites([Vec<(Entity, i32)>; 12]);

#[derive(Resource, Default)]
pub(super) struct SpeciesNeedProfiles {
    pub(super) rows: HashMap<AssetId, SpeciesNeedProfile>,
}

#[derive(Clone, Copy)]
pub(super) struct NeedUpdateRule {
    adjustment: i32,
    pub(super) trigger: Option<u16>,
    pub(super) cessation: Option<u16>,
}

pub(super) struct SpeciesNeedProfile {
    needs: [Option<NeedUpdateRule>; 12],
    pub(super) critical: u16,
    pub(super) good: u16,
}

fn project_species_need_profiles(
    assets: Res<Assets<SpeciesAsset>>,
    collection: Res<SpeciesAssets>,
    mut profiles: ResMut<SpeciesNeedProfiles>,
) {
    if !assets.is_changed() && !collection.is_changed() {
        return;
    }
    profiles.rows.clear();
    let Some(species) = collection.get(&assets) else {
        return;
    };
    for species in species.species() {
        let mut needs = [None; 12];
        let mut critical = [0; 10];
        let mut good = [1000; 10];
        for need in &species.needs {
            let rank = need_rank(&need.kind).unwrap_or(REPRODUCTION_TRIGGER_INDEX);
            needs[rank] = Some(NeedUpdateRule {
                adjustment: need.wellness_adjustment_per_update_q16,
                trigger: need.trigger_wellness_threshold,
                cessation: need.cessation_wellness_threshold,
            });
            if rank < 10 {
                critical[rank] = need.critical_wellness_threshold.unwrap_or(0);
                good[rank] = need.cessation_wellness_threshold.unwrap_or(1000);
            }
        }
        if needs[..11].iter().all(Option::is_some) {
            profiles.rows.insert(
                species.id,
                SpeciesNeedProfile {
                    needs,
                    critical: critical.into_iter().sum::<u16>() / 10,
                    good: good.into_iter().sum::<u16>() / 10,
                },
            );
        }
    }
}

fn initialize_animal_need_latches(
    restored: Query<(Entity, &NeedTriggerState), (With<Animal>, Without<NeedLatch<0>>)>,
    mut commands: Commands,
) {
    for (animal, triggers) in &restored {
        let bit = |index: u16| triggers.0 & (1_u16 << index) != 0;
        commands.entity(animal).insert((
            NeedLatch::<0>(bit(0)),
            NeedLatch::<1>(bit(1)),
            NeedLatch::<2>(bit(2)),
            NeedLatch::<3>(bit(3)),
            NeedLatch::<4>(bit(4)),
            NeedLatch::<5>(bit(5)),
            NeedLatch::<6>(bit(6)),
            NeedLatch::<7>(bit(7)),
            NeedLatch::<8>(bit(8)),
            NeedLatch::<9>(bit(9)),
            NeedLatch::<10>(bit(10)),
            NeedLatch::<11>(bit(11)),
        ));
    }
}

fn advance_animal_need_decay_cadence(
    time: Res<Time<Fixed>>,
    seed: Res<ZooSeed>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    profiles: Res<SpeciesNeedProfiles>,
    mut animals: Query<
        (
            Entity,
            &PersistentId,
            &SpeciesHandle,
            Option<&mut AnimalNeedAdjustmentTimer>,
        ),
        (With<Animal>, With<NeedLatch<0>>, Without<Dead>),
    >,
    mut due: ResMut<DueAnimalNeedDecay>,
    mut commands: Commands,
) {
    due.0.clear();
    let Some(policy) = definitions
        .get(&definition_assets)
        .and_then(|view| view.need_adjustment())
    else {
        return;
    };
    if !policy.enabled {
        return;
    }
    let step_ns = u64::try_from(time.timestep().as_nanos()).unwrap_or(u64::MAX);
    for (animal, persistent_id, species, timer) in &mut animals {
        if !profiles.rows.contains_key(&species.species) {
            continue;
        }
        let advance = |timer: &mut AnimalNeedAdjustmentTimer| {
            openzt2_zoo_state::need_adjustment::advance_need_adjustment_timer(
                &mut timer.remaining_ns,
                step_ns,
                policy.delay_steps,
                &mut timer.random,
            )
        };
        let elapsed = if let Some(mut timer) = timer {
            advance(&mut timer)
        } else {
            let mut timer = AnimalNeedAdjustmentTimer {
                remaining_ns: 0,
                random: DeterministicRng::from_entity(
                    *seed,
                    *persistent_id,
                    RngDomain::AnimalNeeds,
                ),
            };
            let elapsed = advance(&mut timer);
            commands.entity(animal).insert(timer);
            elapsed
        };
        if elapsed {
            due.0.push(animal);
        }
    }
}

fn route_absolute_animal_need_writes(
    mut incoming: MessageReader<SetAnimalNeed>,
    mut routed: ResMut<AbsoluteAnimalNeedWrites>,
) {
    for writes in &mut routed.0 {
        writes.clear();
    }
    for message in incoming.read() {
        let rank = need_rank(&message.need).unwrap_or(REPRODUCTION_TRIGGER_INDEX);
        routed.0[rank].push((message.animal, message.value_q16.clamp(0, MAX_NEED_Q16)));
    }
}

pub(super) fn observe_need_write<const NEED: usize>(
    animal: Entity,
    kind: NeedKind,
    previous: i32,
    next: i32,
    trigger: Option<u16>,
    cessation: Option<u16>,
    latch: Mut<NeedLatch<NEED>>,
    crossings: &mut MessageWriter<NeedThresholdCrossing<NEED>>,
) {
    let latched = if trigger.is_some_and(|threshold| next <= i32::from(threshold) * Q16_ONE) {
        true
    } else if cessation.is_some_and(|threshold| next >= i32::from(threshold) * Q16_ONE) {
        false
    } else {
        latch.0
    };
    latch
        .map_unchanged(|latch| &mut latch.0)
        .set_if_neq(latched);
    if animal_need_crossed_authored_threshold(previous, next, trigger, cessation) {
        crossings.write(NeedThresholdCrossing(NeedChanged {
            animal,
            need: kind,
            value: convert_animal_need_q16_to_permille(next),
        }));
    }
}

macro_rules! need_writes {
    ($decay:ident, $adjust:ident, $message:ty, $value:ty, $kind:path, $index:literal) => {
        fn $decay(
            due: Res<DueAnimalNeedDecay>,
            profiles: Res<SpeciesNeedProfiles>,
            mut animals: Query<
                (&SpeciesHandle, &mut $value, &mut NeedLatch<$index>),
                (With<Animal>, Without<Dead>),
            >,
            mut crossings: MessageWriter<NeedThresholdCrossing<$index>>,
        ) {
            for &animal in &due.0 {
                let Ok((species, value, latch)) = animals.get_mut(animal) else {
                    continue;
                };
                let Some(rule) = profiles
                    .rows
                    .get(&species.species)
                    .and_then(|rules| rules.needs[$index])
                else {
                    continue;
                };
                let previous = value.0;
                let next = clamp_animal_need_q16_after_delta(previous, rule.adjustment);
                value.map_unchanged(|value| &mut value.0).set_if_neq(next);
                observe_need_write(
                    animal,
                    $kind,
                    previous,
                    next,
                    rule.trigger,
                    rule.cessation,
                    latch,
                    &mut crossings,
                );
            }
        }

        fn $adjust(
            mut adjustments: MessageReader<$message>,
            absolute: Res<AbsoluteAnimalNeedWrites>,
            profiles: Res<SpeciesNeedProfiles>,
            mut animals: Query<
                (&SpeciesHandle, &mut $value, &mut NeedLatch<$index>),
                (With<Animal>, Without<Dead>),
            >,
            mut crossings: MessageWriter<NeedThresholdCrossing<$index>>,
        ) {
            let writes = absolute.0[$index]
                .iter()
                .map(|&(animal, value)| (animal, value, true))
                .chain(
                    adjustments
                        .read()
                        .map(|message| (message.animal, message.delta_q16, false)),
                );
            for (animal, operand, absolute) in writes {
                let Ok((species, value, latch)) = animals.get_mut(animal) else {
                    continue;
                };
                let Some(rule) = profiles
                    .rows
                    .get(&species.species)
                    .and_then(|rules| rules.needs[$index])
                else {
                    continue;
                };
                let previous = value.0;
                let next = if absolute {
                    operand
                } else {
                    clamp_animal_need_q16_after_delta(previous, operand)
                };
                value.map_unchanged(|value| &mut value.0).set_if_neq(next);
                observe_need_write(
                    animal,
                    $kind,
                    previous,
                    next,
                    rule.trigger,
                    rule.cessation,
                    latch,
                    &mut crossings,
                );
            }
        }
    };
}

need_writes!(
    decay_hunger,
    adjust_hunger,
    AdjustHunger,
    Hunger,
    NeedKind::Hunger,
    0
);
need_writes!(
    decay_thirst,
    adjust_thirst,
    AdjustThirst,
    Thirst,
    NeedKind::Thirst,
    1
);
need_writes!(
    decay_rest,
    adjust_rest,
    AdjustRest,
    RestNeed,
    NeedKind::Rest,
    2
);
need_writes!(
    decay_privacy,
    adjust_privacy,
    AdjustPrivacy,
    PrivacyNeed,
    NeedKind::Privacy,
    3
);
need_writes!(
    decay_social,
    adjust_social,
    AdjustSocial,
    SocialNeed,
    NeedKind::Social,
    4
);
need_writes!(
    decay_exercise,
    adjust_exercise,
    AdjustExercise,
    ExerciseNeed,
    NeedKind::Exercise,
    5
);
need_writes!(
    decay_stimulation,
    adjust_stimulation,
    AdjustStimulation,
    StimulationNeed,
    NeedKind::Stimulation,
    6
);
need_writes!(
    decay_environment,
    adjust_environment,
    AdjustEnvironment,
    EnvironmentNeed,
    NeedKind::Environment,
    7
);
need_writes!(
    decay_health,
    adjust_health,
    AdjustHealthNeed,
    HealthNeed,
    NeedKind::Health,
    8
);
need_writes!(
    decay_hygiene,
    adjust_hygiene,
    AdjustHygiene,
    HygieneNeed,
    NeedKind::Hygiene,
    9
);
need_writes!(
    decay_bathroom,
    adjust_bathroom,
    AdjustBathroom,
    BathroomNeed,
    NeedKind::Bathroom,
    10
);
need_writes!(
    decay_reproduction,
    adjust_reproduction,
    AdjustReproduction,
    ReproductionNeed,
    NeedKind::Reproduction,
    11
);

#[derive(SystemParam)]
struct NeedObservations<'w, 's, const NEED: usize> {
    latches: Query<'w, 's, (Entity, &'static NeedLatch<NEED>), Changed<NeedLatch<NEED>>>,
    crossings: MessageReader<'w, 's, NeedThresholdCrossing<NEED>>,
}

impl<const NEED: usize> NeedObservations<'_, '_, NEED> {
    fn publish(
        &mut self,
        triggers: &mut Query<&mut NeedTriggerState>,
        changed: &mut MessageWriter<NeedChanged>,
    ) {
        let bit = 1 << NEED;
        for (entity, latch) in &self.latches {
            if let Ok(mut state) = triggers.get_mut(entity) {
                let mask = (state.0 & !bit) | (u16::from(latch.0) << NEED);
                state.set_if_neq(NeedTriggerState(mask));
            }
        }
        changed.write_batch(self.crossings.read().map(|crossing| crossing.0));
    }
}

type AnimalNeedObservations<'w, 's> = (
    NeedObservations<'w, 's, 0>,
    NeedObservations<'w, 's, 1>,
    NeedObservations<'w, 's, 2>,
    NeedObservations<'w, 's, 3>,
    NeedObservations<'w, 's, 4>,
    NeedObservations<'w, 's, 5>,
    NeedObservations<'w, 's, 6>,
    NeedObservations<'w, 's, 7>,
    NeedObservations<'w, 's, 8>,
    NeedObservations<'w, 's, 9>,
    NeedObservations<'w, 's, 10>,
    NeedObservations<'w, 's, 11>,
);

fn publish_animal_need_observations(
    (
        mut hunger,
        mut thirst,
        mut rest,
        mut privacy,
        mut social,
        mut exercise,
        mut stimulation,
        mut environment,
        mut health,
        mut hygiene,
        mut bathroom,
        mut reproduction,
    ): AnimalNeedObservations,
    mut triggers: Query<&mut NeedTriggerState>,
    mut changed: MessageWriter<NeedChanged>,
) {
    hunger.publish(&mut triggers, &mut changed);
    thirst.publish(&mut triggers, &mut changed);
    rest.publish(&mut triggers, &mut changed);
    privacy.publish(&mut triggers, &mut changed);
    social.publish(&mut triggers, &mut changed);
    exercise.publish(&mut triggers, &mut changed);
    stimulation.publish(&mut triggers, &mut changed);
    environment.publish(&mut triggers, &mut changed);
    health.publish(&mut triggers, &mut changed);
    hygiene.publish(&mut triggers, &mut changed);
    bathroom.publish(&mut triggers, &mut changed);
    reproduction.publish(&mut triggers, &mut changed);
}

pub(super) fn register_animal_need_updates(app: &mut App) {
    app.init_resource::<SpeciesNeedProfiles>()
        .init_resource::<DueAnimalNeedDecay>()
        .init_resource::<AbsoluteAnimalNeedWrites>()
        .add_message::<NeedThresholdCrossing<0>>()
        .add_message::<NeedThresholdCrossing<1>>()
        .add_message::<NeedThresholdCrossing<2>>()
        .add_message::<NeedThresholdCrossing<3>>()
        .add_message::<NeedThresholdCrossing<4>>()
        .add_message::<NeedThresholdCrossing<5>>()
        .add_message::<NeedThresholdCrossing<6>>()
        .add_message::<NeedThresholdCrossing<7>>()
        .add_message::<NeedThresholdCrossing<8>>()
        .add_message::<NeedThresholdCrossing<9>>()
        .add_message::<NeedThresholdCrossing<10>>()
        .add_message::<NeedThresholdCrossing<11>>()
        .add_systems(
            FixedUpdate,
            (
                (
                    project_species_need_profiles,
                    initialize_animal_need_latches,
                ),
                advance_animal_need_decay_cadence,
                (
                    decay_hunger,
                    decay_thirst,
                    decay_rest,
                    decay_privacy,
                    decay_social,
                    decay_exercise,
                    decay_stimulation,
                    decay_environment,
                    decay_health,
                    decay_hygiene,
                    decay_bathroom,
                    decay_reproduction,
                ),
            )
                .chain()
                .in_set(FixedGameSet::Clock)
                .run_if(in_state(GamePhase::InGame))
                .run_if(simulation_is_running),
        )
        .add_systems(
            FixedUpdate,
            (
                route_absolute_animal_need_writes,
                (
                    adjust_hunger,
                    adjust_thirst,
                    adjust_rest,
                    adjust_privacy,
                    adjust_social,
                    adjust_exercise,
                    adjust_stimulation,
                    adjust_environment,
                    adjust_health,
                    adjust_hygiene,
                    adjust_bathroom,
                    adjust_reproduction,
                ),
                publish_animal_need_observations.before(super::systems::update_need_points),
            )
                .chain()
                .in_set(FixedGameSet::Act)
                .run_if(in_state(GamePhase::InGame))
                .run_if(simulation_is_running),
        );
}
