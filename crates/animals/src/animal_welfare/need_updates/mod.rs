use bevy::{
    ecs::{entity::EntityLocation, query::QueryData, system::SystemParam},
    platform::collections::HashMap,
    prelude::*,
    utils::Parallel,
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
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    animal_health::types::Dead,
    animal_lifecycle::types::{Animal, SpeciesHandle},
    animal_welfare::types::*,
};

use super::need_timing_types::NeedAdjustmentCadence;
use super::{calculations::*, need_latch_types::NeedLatch, systems::need_rank};

#[derive(Message)]
pub(super) struct NeedThresholdCrossing<const NEED: usize>(pub(super) NeedChanged);

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

#[derive(QueryData)]
#[query_data(mutable)]
struct AnimalNeedDecay {
    animal: Entity,
    id: &'static PersistentId,
    member: &'static WorldMember,
    species: &'static SpeciesHandle,
    timer: Option<&'static mut NeedAdjustmentCadence>,
    hunger: (&'static mut Hunger, &'static mut NeedLatch<0>),
    thirst: (&'static mut Thirst, &'static mut NeedLatch<1>),
    rest: (&'static mut RestNeed, &'static mut NeedLatch<2>),
    privacy: (&'static mut PrivacyNeed, &'static mut NeedLatch<3>),
    social: (&'static mut SocialNeed, &'static mut NeedLatch<4>),
    exercise: (&'static mut ExerciseNeed, &'static mut NeedLatch<5>),
    stimulation: (&'static mut StimulationNeed, &'static mut NeedLatch<6>),
    environment: (&'static mut EnvironmentNeed, &'static mut NeedLatch<7>),
    health: (&'static mut HealthNeed, &'static mut NeedLatch<8>),
    hygiene: (&'static mut HygieneNeed, &'static mut NeedLatch<9>),
    bathroom: (&'static mut BathroomNeed, &'static mut NeedLatch<10>),
    reproduction: Option<(&'static mut ReproductionNeed, &'static mut NeedLatch<11>)>,
}

fn decay_due_animal_needs(
    time: Res<Time<Fixed>>,
    seeds: Query<&ZooSeed>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    profiles: Res<SpeciesNeedProfiles>,
    mut animals: Query<(EntityLocation, AnimalNeedDecay), (With<Animal>, Without<Dead>)>,
    parallel: ParallelCommands,
    mut crossings: NeedDecayCrossingWriters,
) {
    let Some(policy) = definitions
        .get(&definition_assets)
        .and_then(|view| view.need_adjustment())
        .filter(|policy| policy.enabled)
    else {
        return;
    };
    let step = time.timestep();
    let mut pending = Parallel::<Vec<(usize, usize, usize, NeedChanged)>>::default();
    animals.par_iter_mut().for_each(|(location, row)| {
        let Some(profile) = profiles.rows.get(&row.species.species) else {
            return;
        };
        let due = if let Some(mut timer) = row.timer {
            timer.tick(step, policy.delay_steps)
        } else {
            let Ok(seed) = seeds.get(row.member.root) else {
                return;
            };
            let mut timer = NeedAdjustmentCadence::from_saved_remaining(
                0,
                DeterministicRng::from_entity(*seed, *row.id, RngDomain::AnimalNeeds),
            );
            let due = timer.tick(step, policy.delay_steps);
            parallel.command_scope(|mut commands| {
                commands.entity(row.animal).insert(timer);
            });
            due
        };
        if !due {
            return;
        }
        macro_rules! decay {
            ($rank:literal, $kind:ident, ($need:expr, $latch:expr)) => {
                if let Some(rule) = profile.needs[$rank] {
                    let need = $need.map_unchanged(|need| &mut need.0);
                    let next = clamp_animal_need_q16_after_delta(*need, rule.adjustment);
                    if let Some(crossing) =
                        write_need(row.animal, NeedKind::$kind, next, rule, need, $latch)
                    {
                        pending.scope(|messages| {
                            messages.push((
                                $rank,
                                location.table_id.as_u32() as usize,
                                location.table_row.index(),
                                crossing,
                            ))
                        });
                    }
                }
            };
        }
        decay!(0, Hunger, (row.hunger.0, row.hunger.1));
        decay!(1, Thirst, (row.thirst.0, row.thirst.1));
        decay!(2, Rest, (row.rest.0, row.rest.1));
        decay!(3, Privacy, (row.privacy.0, row.privacy.1));
        decay!(4, Social, (row.social.0, row.social.1));
        decay!(5, Exercise, (row.exercise.0, row.exercise.1));
        decay!(6, Stimulation, (row.stimulation.0, row.stimulation.1));
        decay!(7, Environment, (row.environment.0, row.environment.1));
        decay!(8, Health, (row.health.0, row.health.1));
        decay!(9, Hygiene, (row.hygiene.0, row.hygiene.1));
        decay!(10, Bathroom, (row.bathroom.0, row.bathroom.1));
        if let Some((need, latch)) = row.reproduction {
            decay!(11, Reproduction, (need, latch));
        }
    });
    let mut sorted = pending.drain().collect::<Vec<_>>();
    sorted.sort_unstable_by_key(|&(rank, table, row, _)| (rank, table, row));
    for (rank, _, _, message) in sorted {
        match rank {
            0 => {
                crossings.hunger.write(NeedThresholdCrossing(message));
            }
            1 => {
                crossings.thirst.write(NeedThresholdCrossing(message));
            }
            2 => {
                crossings.rest.write(NeedThresholdCrossing(message));
            }
            3 => {
                crossings.privacy.write(NeedThresholdCrossing(message));
            }
            4 => {
                crossings.social.write(NeedThresholdCrossing(message));
            }
            5 => {
                crossings.exercise.write(NeedThresholdCrossing(message));
            }
            6 => {
                crossings.stimulation.write(NeedThresholdCrossing(message));
            }
            7 => {
                crossings.environment.write(NeedThresholdCrossing(message));
            }
            8 => {
                crossings.health.write(NeedThresholdCrossing(message));
            }
            9 => {
                crossings.hygiene.write(NeedThresholdCrossing(message));
            }
            10 => {
                crossings.bathroom.write(NeedThresholdCrossing(message));
            }
            11 => {
                crossings.reproduction.write(NeedThresholdCrossing(message));
            }
            _ => unreachable!(),
        }
    }
}

fn write_need<const NEED: usize>(
    animal: Entity,
    kind: NeedKind,
    next: i32,
    rule: NeedUpdateRule,
    mut need: Mut<i32>,
    latch: Mut<NeedLatch<NEED>>,
) -> Option<NeedChanged> {
    let previous = *need;
    need.set_if_neq(next);
    observe_need_write(
        animal,
        kind,
        previous,
        next,
        rule.trigger,
        rule.cessation,
        latch,
    )
}

pub(super) fn observe_need_write<const NEED: usize>(
    animal: Entity,
    kind: NeedKind,
    previous: i32,
    next: i32,
    trigger: Option<u16>,
    cessation: Option<u16>,
    latch: Mut<NeedLatch<NEED>>,
) -> Option<NeedChanged> {
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
    animal_need_crossed_authored_threshold(previous, next, trigger, cessation).then(|| {
        NeedChanged {
            animal,
            need: kind,
            value: convert_animal_need_q16_to_permille(next),
        }
    })
}

#[derive(Message)]
struct SetNeed<const NEED: usize> {
    animal: Entity,
    value: i32,
}

#[derive(SystemParam)]
struct NeedDecayCrossingWriters<'w> {
    hunger: MessageWriter<'w, NeedThresholdCrossing<0>>,
    thirst: MessageWriter<'w, NeedThresholdCrossing<1>>,
    rest: MessageWriter<'w, NeedThresholdCrossing<2>>,
    privacy: MessageWriter<'w, NeedThresholdCrossing<3>>,
    social: MessageWriter<'w, NeedThresholdCrossing<4>>,
    exercise: MessageWriter<'w, NeedThresholdCrossing<5>>,
    stimulation: MessageWriter<'w, NeedThresholdCrossing<6>>,
    environment: MessageWriter<'w, NeedThresholdCrossing<7>>,
    health: MessageWriter<'w, NeedThresholdCrossing<8>>,
    hygiene: MessageWriter<'w, NeedThresholdCrossing<9>>,
    bathroom: MessageWriter<'w, NeedThresholdCrossing<10>>,
    reproduction: MessageWriter<'w, NeedThresholdCrossing<11>>,
}

#[derive(SystemParam)]
struct AbsoluteNeedWriters<'w> {
    hunger: MessageWriter<'w, SetNeed<0>>,
    thirst: MessageWriter<'w, SetNeed<1>>,
    rest: MessageWriter<'w, SetNeed<2>>,
    privacy: MessageWriter<'w, SetNeed<3>>,
    social: MessageWriter<'w, SetNeed<4>>,
    exercise: MessageWriter<'w, SetNeed<5>>,
    stimulation: MessageWriter<'w, SetNeed<6>>,
    environment: MessageWriter<'w, SetNeed<7>>,
    health: MessageWriter<'w, SetNeed<8>>,
    hygiene: MessageWriter<'w, SetNeed<9>>,
    bathroom: MessageWriter<'w, SetNeed<10>>,
    reproduction: MessageWriter<'w, SetNeed<11>>,
}

fn route_absolute_animal_need_writes(
    mut incoming: MessageReader<SetAnimalNeed>,
    mut lanes: AbsoluteNeedWriters,
) {
    for message in incoming.read() {
        let animal = message.animal;
        let value = message.value_q16.clamp(0, MAX_NEED_Q16);
        match message.need {
            NeedKind::Hunger => {
                lanes.hunger.write(SetNeed { animal, value });
            }
            NeedKind::Thirst => {
                lanes.thirst.write(SetNeed { animal, value });
            }
            NeedKind::Rest => {
                lanes.rest.write(SetNeed { animal, value });
            }
            NeedKind::Privacy => {
                lanes.privacy.write(SetNeed { animal, value });
            }
            NeedKind::Social => {
                lanes.social.write(SetNeed { animal, value });
            }
            NeedKind::Exercise => {
                lanes.exercise.write(SetNeed { animal, value });
            }
            NeedKind::Stimulation => {
                lanes.stimulation.write(SetNeed { animal, value });
            }
            NeedKind::Environment => {
                lanes.environment.write(SetNeed { animal, value });
            }
            NeedKind::Health => {
                lanes.health.write(SetNeed { animal, value });
            }
            NeedKind::Hygiene => {
                lanes.hygiene.write(SetNeed { animal, value });
            }
            NeedKind::Bathroom => {
                lanes.bathroom.write(SetNeed { animal, value });
            }
            NeedKind::Reproduction => {
                lanes.reproduction.write(SetNeed { animal, value });
            }
        }
    }
}

macro_rules! need_writes {
    ($adjust:ident, $message:ty, $value:ty, $kind:path, $rank:literal) => {
        fn $adjust(
            mut adjustments: MessageReader<$message>,
            mut absolute: MessageReader<SetNeed<$rank>>,
            profiles: Res<SpeciesNeedProfiles>,
            mut animals: Query<
                (&SpeciesHandle, &mut $value, &mut NeedLatch<$rank>),
                (With<Animal>, Without<Dead>),
            >,
            mut crossings: MessageWriter<NeedThresholdCrossing<$rank>>,
        ) {
            let writes = absolute
                .read()
                .map(|message| (message.animal, message.value, true))
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
                    .and_then(|profile| profile.needs[$rank])
                else {
                    continue;
                };
                let next = if absolute {
                    operand
                } else {
                    clamp_animal_need_q16_after_delta(value.0, operand)
                };
                if let Some(crossing) = write_need(
                    animal,
                    $kind,
                    next,
                    rule,
                    value.map_unchanged(|value| &mut value.0),
                    latch,
                ) {
                    crossings.write(NeedThresholdCrossing(crossing));
                }
            }
        }
    };
}

need_writes!(adjust_hunger, AdjustHunger, Hunger, NeedKind::Hunger, 0);
need_writes!(adjust_thirst, AdjustThirst, Thirst, NeedKind::Thirst, 1);
need_writes!(adjust_rest, AdjustRest, RestNeed, NeedKind::Rest, 2);
need_writes!(
    adjust_privacy,
    AdjustPrivacy,
    PrivacyNeed,
    NeedKind::Privacy,
    3
);
need_writes!(adjust_social, AdjustSocial, SocialNeed, NeedKind::Social, 4);
need_writes!(
    adjust_exercise,
    AdjustExercise,
    ExerciseNeed,
    NeedKind::Exercise,
    5
);
need_writes!(
    adjust_stimulation,
    AdjustStimulation,
    StimulationNeed,
    NeedKind::Stimulation,
    6
);
need_writes!(
    adjust_environment,
    AdjustEnvironment,
    EnvironmentNeed,
    NeedKind::Environment,
    7
);
need_writes!(
    adjust_health,
    AdjustHealthNeed,
    HealthNeed,
    NeedKind::Health,
    8
);
need_writes!(
    adjust_hygiene,
    AdjustHygiene,
    HygieneNeed,
    NeedKind::Hygiene,
    9
);
need_writes!(
    adjust_bathroom,
    AdjustBathroom,
    BathroomNeed,
    NeedKind::Bathroom,
    10
);
need_writes!(
    adjust_reproduction,
    AdjustReproduction,
    ReproductionNeed,
    NeedKind::Reproduction,
    11
);

fn publish_animal_need_crossings(
    mut hunger: MessageReader<NeedThresholdCrossing<0>>,
    mut thirst: MessageReader<NeedThresholdCrossing<1>>,
    mut rest: MessageReader<NeedThresholdCrossing<2>>,
    mut privacy: MessageReader<NeedThresholdCrossing<3>>,
    mut social: MessageReader<NeedThresholdCrossing<4>>,
    mut exercise: MessageReader<NeedThresholdCrossing<5>>,
    mut stimulation: MessageReader<NeedThresholdCrossing<6>>,
    mut environment: MessageReader<NeedThresholdCrossing<7>>,
    mut health: MessageReader<NeedThresholdCrossing<8>>,
    mut hygiene: MessageReader<NeedThresholdCrossing<9>>,
    mut bathroom: MessageReader<NeedThresholdCrossing<10>>,
    mut reproduction: MessageReader<NeedThresholdCrossing<11>>,
    mut changed: MessageWriter<NeedChanged>,
) {
    changed.write_batch(hunger.read().map(|crossing| crossing.0));
    changed.write_batch(thirst.read().map(|crossing| crossing.0));
    changed.write_batch(rest.read().map(|crossing| crossing.0));
    changed.write_batch(privacy.read().map(|crossing| crossing.0));
    changed.write_batch(social.read().map(|crossing| crossing.0));
    changed.write_batch(exercise.read().map(|crossing| crossing.0));
    changed.write_batch(stimulation.read().map(|crossing| crossing.0));
    changed.write_batch(environment.read().map(|crossing| crossing.0));
    changed.write_batch(health.read().map(|crossing| crossing.0));
    changed.write_batch(hygiene.read().map(|crossing| crossing.0));
    changed.write_batch(bathroom.read().map(|crossing| crossing.0));
    changed.write_batch(reproduction.read().map(|crossing| crossing.0));
}

pub(super) fn register_animal_need_updates(app: &mut App) {
    app.init_resource::<SpeciesNeedProfiles>()
        .add_message::<SetNeed<0>>()
        .add_message::<NeedThresholdCrossing<0>>()
        .add_message::<SetNeed<1>>()
        .add_message::<NeedThresholdCrossing<1>>()
        .add_message::<SetNeed<2>>()
        .add_message::<NeedThresholdCrossing<2>>()
        .add_message::<SetNeed<3>>()
        .add_message::<NeedThresholdCrossing<3>>()
        .add_message::<SetNeed<4>>()
        .add_message::<NeedThresholdCrossing<4>>()
        .add_message::<SetNeed<5>>()
        .add_message::<NeedThresholdCrossing<5>>()
        .add_message::<SetNeed<6>>()
        .add_message::<NeedThresholdCrossing<6>>()
        .add_message::<SetNeed<7>>()
        .add_message::<NeedThresholdCrossing<7>>()
        .add_message::<SetNeed<8>>()
        .add_message::<NeedThresholdCrossing<8>>()
        .add_message::<SetNeed<9>>()
        .add_message::<NeedThresholdCrossing<9>>()
        .add_message::<SetNeed<10>>()
        .add_message::<NeedThresholdCrossing<10>>()
        .add_message::<SetNeed<11>>()
        .add_message::<NeedThresholdCrossing<11>>()
        .add_systems(
            FixedUpdate,
            (
                project_species_need_profiles,
                decay_due_animal_needs.after(project_species_need_profiles),
            )
                .in_set(FixedGameSet::Clock)
                .run_if(in_state(GamePhase::InGame))
                .run_if(simulation_is_running),
        )
        .add_systems(
            FixedUpdate,
            (
                route_absolute_animal_need_writes,
                (
                    adjust_hunger.before(super::systems::update_need_points),
                    adjust_thirst.before(super::systems::update_need_points),
                    adjust_rest.before(super::systems::update_need_points),
                    adjust_privacy.before(super::systems::update_need_points),
                    adjust_social.before(super::systems::update_need_points),
                    adjust_exercise.before(super::systems::update_need_points),
                    adjust_stimulation.before(super::systems::update_need_points),
                    adjust_environment.before(super::systems::update_need_points),
                    adjust_health.before(super::systems::update_need_points),
                    adjust_hygiene.before(super::systems::update_need_points),
                    adjust_bathroom.before(super::systems::update_need_points),
                    adjust_reproduction.before(super::systems::update_need_points),
                ),
                publish_animal_need_crossings.before(super::systems::update_need_points),
            )
                .chain()
                .in_set(FixedGameSet::Act)
                .run_if(in_state(GamePhase::InGame))
                .run_if(simulation_is_running),
        );
}
