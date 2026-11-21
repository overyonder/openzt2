use bevy::prelude::*;
use openzt2_game_data::species::{NeedKind, SpeciesNeed};

use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_world_state::simulation_time::deterministic_random_stream::RngDomain;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;

use super::{
    calculations::{
        animal_need_crossed_authored_threshold, clamp_animal_need_q16_after_delta,
        convert_animal_need_q16_to_permille, welfare_from_need_points,
    },
    types::*,
};

fn need_rank(kind: &NeedKind) -> Option<usize> {
    Some(match kind {
        NeedKind::Hunger => 0,
        NeedKind::Thirst => 1,
        NeedKind::Rest => 2,
        NeedKind::Privacy => 3,
        NeedKind::Social => 4,
        NeedKind::Exercise => 5,
        NeedKind::Stimulation => 6,
        NeedKind::Environment => 7,
        NeedKind::Health => 8,
        NeedKind::Hygiene => 9,
        NeedKind::Bathroom => 10,
        NeedKind::Reproduction => return None,
    })
}

fn species_reproduction_need<'a>(
    index: &'a SpeciesAssets,
    assets: &'a Assets<SpeciesAsset>,
    handle: &SpeciesHandle,
) -> Option<&'a SpeciesNeed> {
    index
        .get(assets)?
        .find(handle.species)?
        .needs
        .iter()
        .find(|need| need.kind == NeedKind::Reproduction)
}

fn species_needs<'a>(
    index: &'a SpeciesAssets,
    assets: &'a Assets<SpeciesAsset>,
    handle: &SpeciesHandle,
) -> Option<[&'a SpeciesNeed; 11]> {
    let species = index.get(assets)?.find(handle.species)?;
    let rows = &species.needs;
    let mut found: [Option<&SpeciesNeed>; 11] = [None; 11];
    for row in rows {
        if let Some(rank) = need_rank(&row.kind) {
            found[rank] = Some(row);
        }
    }
    Some([
        found[0]?, found[1]?, found[2]?, found[3]?, found[4]?, found[5]?, found[6]?, found[7]?,
        found[8]?, found[9]?, found[10]?,
    ])
}

pub(crate) fn initialize_animal_needs(
    mut commands: Commands,
    zoo_seed: Res<ZooSeed>,
    assets: Res<Assets<SpeciesAsset>>,
    index: Res<SpeciesAssets>,
    animals: Query<
        (
            Entity,
            &PersistentId,
            &SpeciesHandle,
            Option<
                &crate::animal_lifecycle::scripted_animal_spawning::ScriptedAnimalInitialization,
            >,
        ),
        (With<Animal>, Without<AnimalWelfare>, Without<Dead>),
    >,
) {
    for (entity, persistent_id, species, scripted) in &animals {
        let Some(needs) = species_needs(&index, &assets, species) else {
            continue;
        };
        let mut random =
            DeterministicRng::from_entity(*zoo_seed, *persistent_id, RngDomain::AnimalNeeds);
        let mut sample = |need: &SpeciesNeed| {
            let minimum = need.initial_wellness_permille_range[0];
            let maximum = need.initial_wellness_permille_range[1].max(minimum);
            let width = u32::from(maximum - minimum) + 1;
            let sampled = minimum + random.range_u32(width).unwrap_or_default() as u16;
            i32::from(sampled) * Q16_ONE
        };
        let mut initial = needs.map(&mut sample);
        // Sampled after the welfare needs so their streams stay as they were.
        let reproduction_row = species_reproduction_need(&index, &assets, species);
        let mut reproduction = reproduction_row.map(&mut sample);
        if let Some(scripted) = scripted {
            for (kind, value) in &scripted.needs {
                let value = (*value).clamp(0, MAX_NEED_Q16);
                match need_rank(kind) {
                    Some(rank) => initial[rank] = value,
                    None => reproduction = reproduction.map(|_| value),
                }
            }
        }
        // The next assessment uses live membership and species population.
        let suitability = HabitatSuitability::default();
        let (value, band) = welfare_from_need_points(&NeedPoints::default());
        let mut trigger_state = NeedTriggerState::default();
        for (index, need) in needs.iter().enumerate() {
            trigger_state.observe(
                index,
                initial[index],
                need.trigger_wellness_threshold,
                need.cessation_wellness_threshold,
            );
        }
        if let (Some(row), Some(value)) = (reproduction_row, reproduction) {
            trigger_state.observe(
                REPRODUCTION_TRIGGER_INDEX,
                value,
                row.trigger_wellness_threshold,
                row.cessation_wellness_threshold,
            );
            commands.entity(entity).insert(ReproductionNeed(value));
        }
        commands.entity(entity).insert((
            Hunger(initial[0]),
            Thirst(initial[1]),
            RestNeed(initial[2]),
            PrivacyNeed(initial[3]),
            SocialNeed(initial[4]),
            ExerciseNeed(initial[5]),
            StimulationNeed(initial[6]),
            EnvironmentNeed(initial[7]),
            HealthNeed(initial[8]),
        ));
        commands.entity(entity).insert((
            HygieneNeed(initial[9]),
            BathroomNeed(initial[10]),
            trigger_state,
            suitability,
            AnimalWelfare(value),
            CumulativeNeedPoints::default(),
            NeedPoints::default(),
            band,
        ));
        if scripted.is_some() {
            commands.entity(entity).remove::<crate::animal_lifecycle::scripted_animal_spawning::ScriptedAnimalInitialization>();
        }
    }
}

// ponytail: `animal.xml`'s `f_NeedPointUpdateInterval`, `f_NeedPointGoodDelta`,
// `f_NeedPointBadDelta` and `f_NeedPointMax`; read them from the animal
// family if a species ever authors its own.
const NEED_POINT_UPDATE_SECONDS: f32 = 3.0;
const NEED_POINT_DELTA: f32 = 0.2;
const NEED_POINT_MAX: f32 = 100.0;

/// One need-point update: the branch matching the animal's needs grows. When
/// the animal's state flips, the other branch is cleared first, as in the
/// original (`zt.exe` 0x61478f).
pub(super) fn step_need_points(points: &mut NeedPoints, unmet: bool) {
    let (grows, cleared) = if unmet {
        (&mut points.bad, &mut points.good)
    } else {
        (&mut points.good, &mut points.bad)
    };
    if points.unmet != unmet {
        *cleared = 0.0;
    }
    *grows = (*grows + NEED_POINT_DELTA).min(NEED_POINT_MAX);
    points.unmet = unmet;
}

/// Whether one of the animal's basic needs is at its critical threshold. As in
/// the original (`zt.exe` 0x614a2f), `Advanced` needs such as social or
/// stimulation don't count.
fn has_critical_need(values: [i32; 11], needs: [&SpeciesNeed; 11]) -> bool {
    values.into_iter().zip(needs).any(|(value, need)| {
        !need.advanced
            && need
                .critical_wellness_threshold
                .is_some_and(|threshold| convert_animal_need_q16_to_permille(value) <= threshold)
    })
}

/// Every update an animal with a critical need gains bad need points, and an
/// animal without one good points. A change of state updates at once.
#[allow(clippy::type_complexity)]
pub(super) fn update_need_points(
    time: Res<Time<Fixed>>,
    assets: Res<Assets<SpeciesAsset>>,
    index: Res<SpeciesAssets>,
    mut animals: Query<
        (
            Entity,
            &SpeciesHandle,
            (
                &Hunger,
                &Thirst,
                &RestNeed,
                &PrivacyNeed,
                &SocialNeed,
                &ExerciseNeed,
                &StimulationNeed,
                &EnvironmentNeed,
                &HealthNeed,
                &HygieneNeed,
                &BathroomNeed,
            ),
            &mut NeedPoints,
        ),
        (With<Animal>, Without<Dead>),
    >,
    mut outcomes: MessageWriter<AccumulateNeedPoints>,
) {
    for (animal, species, needs, mut points) in &mut animals {
        let Some(rows) = species_needs(&index, &assets, species) else {
            continue;
        };
        let (
            hunger,
            thirst,
            rest,
            privacy,
            social,
            exercise,
            stimulation,
            environment,
            health,
            hygiene,
            bathroom,
        ) = needs;
        let unmet = has_critical_need(
            [
                hunger.0,
                thirst.0,
                rest.0,
                privacy.0,
                social.0,
                exercise.0,
                stimulation.0,
                environment.0,
                health.0,
                hygiene.0,
                bathroom.0,
            ],
            rows,
        );
        points.seconds_until_update -= time.delta_secs();
        if points.seconds_until_update > 0.0 && unmet == points.unmet {
            continue;
        }
        points.seconds_until_update = NEED_POINT_UPDATE_SECONDS;
        step_need_points(&mut points, unmet);
        outcomes.write(AccumulateNeedPoints {
            animal,
            good: if unmet { 0.0 } else { NEED_POINT_DELTA },
            bad: if unmet { NEED_POINT_DELTA } else { 0.0 },
        });
    }
}

/// Add need-point outcomes to the lifetime counters without clamping them.
pub(super) fn accumulate_need_points(
    mut outcomes: MessageReader<AccumulateNeedPoints>,
    mut animals: Query<&mut CumulativeNeedPoints, (With<Animal>, Without<Dead>)>,
) {
    for outcome in outcomes.read() {
        let Ok(mut cumulative) = animals.get_mut(outcome.animal) else {
            continue;
        };
        cumulative.good += outcome.good;
        cumulative.bad += outcome.bad;
    }
}

fn rate_result(value: i32, row: &SpeciesNeed) -> (i32, bool) {
    let next = clamp_animal_need_q16_after_delta(value, row.wellness_adjustment_per_update_q16);
    let crossed = animal_need_crossed_authored_threshold(
        value,
        next,
        row.trigger_wellness_threshold,
        row.cessation_wellness_threshold,
    );
    (next, crossed)
}

#[allow(clippy::type_complexity)]
#[allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems receive resource guards by value"
)]
pub(super) fn decay_animal_needs(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    seed: Res<ZooSeed>,
    (definitions, definition_assets): (
        Res<openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions>,
        Res<Assets<openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset>>,
    ),
    (assets, index): (Res<Assets<SpeciesAsset>>, Res<SpeciesAssets>),
    mut animals: Query<
        (
            Entity,
            &PersistentId,
            &SpeciesHandle,
            (
                &mut Hunger,
                &mut Thirst,
                &mut RestNeed,
                &mut PrivacyNeed,
                &mut SocialNeed,
                &mut ExerciseNeed,
                &mut StimulationNeed,
                &mut EnvironmentNeed,
                &mut HealthNeed,
                &mut HygieneNeed,
                &mut BathroomNeed,
            ),
            &mut NeedTriggerState,
            Option<&mut AnimalNeedAdjustmentTimer>,
            Option<&mut ReproductionNeed>,
        ),
        (With<Animal>, Without<Dead>),
    >,
    mut changed: MessageWriter<NeedChanged>,
) {
    let Some(definitions) = definitions.get(&definition_assets) else {
        return;
    };
    let Some(policy) = definitions.need_adjustment() else {
        return;
    };
    if !policy.enabled {
        return;
    }
    let step_ns = u64::try_from(time.timestep().as_nanos()).unwrap_or(u64::MAX);
    for (
        entity,
        persistent_id,
        species,
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
        ),
        mut triggers,
        timer,
        reproduction,
    ) in &mut animals
    {
        let Some(needs) = species_needs(&index, &assets, species) else {
            continue;
        };
        let mut state = timer
            .as_deref()
            .copied()
            .unwrap_or_else(|| AnimalNeedAdjustmentTimer {
                remaining_ns: 0,
                random: DeterministicRng::from_entity(
                    *seed,
                    *persistent_id,
                    RngDomain::AnimalNeeds,
                ),
            });
        let due = openzt2_zoo_state::need_adjustment::advance_need_adjustment_timer(
            &mut state.remaining_ns,
            step_ns,
            policy.delay_steps,
            &mut state.random,
        );
        if let Some(mut timer) = timer {
            *timer = state;
        } else {
            commands.entity(entity).insert(state);
        }
        if !due {
            continue;
        }
        let values = [
            &mut hunger.0,
            &mut thirst.0,
            &mut rest.0,
            &mut privacy.0,
            &mut social.0,
            &mut exercise.0,
            &mut stimulation.0,
            &mut environment.0,
            &mut health.0,
            &mut hygiene.0,
            &mut bathroom.0,
        ];
        for (index, (value, need)) in values.into_iter().zip(needs).enumerate() {
            let (next, crossed) = rate_result(*value, need);
            *value = next;
            triggers.observe(
                index,
                next,
                need.trigger_wellness_threshold,
                need.cessation_wellness_threshold,
            );
            if crossed {
                changed.write(NeedChanged {
                    animal: entity,
                    need: need.kind,
                    value: convert_animal_need_q16_to_permille(next),
                });
            }
        }
        if let (Some(mut reproduction), Some(need)) = (
            reproduction,
            species_reproduction_need(&index, &assets, species),
        ) {
            let (next, crossed) = rate_result(reproduction.0, need);
            reproduction.0 = next;
            triggers.observe(
                REPRODUCTION_TRIGGER_INDEX,
                next,
                need.trigger_wellness_threshold,
                need.cessation_wellness_threshold,
            );
            if crossed {
                changed.write(NeedChanged {
                    animal: entity,
                    need: need.kind,
                    value: convert_animal_need_q16_to_permille(next),
                });
            }
        }
    }
}

/// Animals from saves made before the reproduction need start it from its
/// authored initial range.
pub(crate) fn initialize_missing_reproduction_needs(
    mut commands: Commands,
    zoo_seed: Res<ZooSeed>,
    assets: Res<Assets<SpeciesAsset>>,
    index: Res<SpeciesAssets>,
    animals: Query<
        (Entity, &PersistentId, &SpeciesHandle),
        (
            With<Animal>,
            With<AnimalWelfare>,
            Without<ReproductionNeed>,
            Without<Dead>,
        ),
    >,
) {
    for (entity, persistent_id, species) in &animals {
        let Some(need) = species_reproduction_need(&index, &assets, species) else {
            continue;
        };
        let mut random =
            DeterministicRng::from_entity(*zoo_seed, *persistent_id, RngDomain::AnimalNeeds);
        let minimum = need.initial_wellness_permille_range[0];
        let maximum = need.initial_wellness_permille_range[1].max(minimum);
        let sampled = minimum
            + random
                .range_u32(u32::from(maximum - minimum) + 1)
                .unwrap_or_default() as u16;
        commands
            .entity(entity)
            .insert(ReproductionNeed(i32::from(sampled) * Q16_ONE));
    }
}

/// `AdjustReproduction`, as the welfare needs' adjustment systems apply theirs.
pub(super) fn adjust_reproduction(
    mut messages: MessageReader<AdjustReproduction>,
    assets: Res<Assets<SpeciesAsset>>,
    index: Res<SpeciesAssets>,
    mut animals: Query<
        (&SpeciesHandle, &mut ReproductionNeed, &mut NeedTriggerState),
        (With<Animal>, Without<Dead>),
    >,
    mut changed: MessageWriter<NeedChanged>,
) {
    for message in messages.read() {
        let Ok((species, mut value, mut triggers)) = animals.get_mut(message.animal) else {
            continue;
        };
        let Some(need) = species_reproduction_need(&index, &assets, species) else {
            continue;
        };
        let old = value.0;
        let next = clamp_animal_need_q16_after_delta(old, message.delta_q16);
        value.0 = next;
        triggers.observe(
            REPRODUCTION_TRIGGER_INDEX,
            next,
            need.trigger_wellness_threshold,
            need.cessation_wellness_threshold,
        );
        if animal_need_crossed_authored_threshold(
            old,
            next,
            need.trigger_wellness_threshold,
            need.cessation_wellness_threshold,
        ) {
            changed.write(NeedChanged {
                animal: message.animal,
                need: NeedKind::Reproduction,
                value: convert_animal_need_q16_to_permille(next),
            });
        }
    }
}

macro_rules! adjustment_system {
    ($function:ident, $message:ty, $component:ty, $index:expr, $kind:expr) => {
        pub(super) fn $function(
            mut messages: MessageReader<$message>,
            assets: Res<Assets<SpeciesAsset>>,
            index: Res<SpeciesAssets>,
            mut animals: Query<
                (&SpeciesHandle, &mut $component, &mut NeedTriggerState),
                (With<Animal>, Without<Dead>),
            >,
            mut changed: MessageWriter<NeedChanged>,
        ) {
            for message in messages.read() {
                let Ok((species, mut value, mut triggers)) = animals.get_mut(message.animal) else {
                    continue;
                };
                let Some(needs) = species_needs(&index, &assets, species) else {
                    continue;
                };
                let old = value.0;
                let next = clamp_animal_need_q16_after_delta(old, message.delta_q16);
                if next != old {
                    value.0 = next;
                }
                triggers.observe(
                    $index,
                    next,
                    needs[$index].trigger_wellness_threshold,
                    needs[$index].cessation_wellness_threshold,
                );
                if animal_need_crossed_authored_threshold(
                    old,
                    next,
                    needs[$index].trigger_wellness_threshold,
                    needs[$index].cessation_wellness_threshold,
                ) {
                    changed.write(NeedChanged {
                        animal: message.animal,
                        need: $kind,
                        value: convert_animal_need_q16_to_permille(next),
                    });
                }
            }
        }
    };
}

adjustment_system!(adjust_hunger, AdjustHunger, Hunger, 0, NeedKind::Hunger);
adjustment_system!(adjust_thirst, AdjustThirst, Thirst, 1, NeedKind::Thirst);
adjustment_system!(adjust_rest, AdjustRest, RestNeed, 2, NeedKind::Rest);
adjustment_system!(
    adjust_privacy,
    AdjustPrivacy,
    PrivacyNeed,
    3,
    NeedKind::Privacy
);
adjustment_system!(adjust_social, AdjustSocial, SocialNeed, 4, NeedKind::Social);
adjustment_system!(
    adjust_exercise,
    AdjustExercise,
    ExerciseNeed,
    5,
    NeedKind::Exercise
);
adjustment_system!(
    adjust_stimulation,
    AdjustStimulation,
    StimulationNeed,
    6,
    NeedKind::Stimulation
);
adjustment_system!(
    adjust_environment,
    AdjustEnvironment,
    EnvironmentNeed,
    7,
    NeedKind::Environment
);
adjustment_system!(
    adjust_health_need,
    AdjustHealthNeed,
    HealthNeed,
    8,
    NeedKind::Health
);

adjustment_system!(
    adjust_hygiene,
    AdjustHygiene,
    HygieneNeed,
    9,
    NeedKind::Hygiene
);

adjustment_system!(
    adjust_bathroom,
    AdjustBathroom,
    BathroomNeed,
    10,
    NeedKind::Bathroom
);

pub(super) fn set_animal_need(
    mut messages: MessageReader<SetAnimalNeed>,
    mut reproduction: MessageWriter<AdjustReproduction>,
    reproduction_needs: Query<&ReproductionNeed>,
    assets: Res<Assets<SpeciesAsset>>,
    index: Res<SpeciesAssets>,
    mut animals: Query<
        (
            &SpeciesHandle,
            &mut Hunger,
            &mut Thirst,
            &mut RestNeed,
            &mut PrivacyNeed,
            &mut SocialNeed,
            &mut ExerciseNeed,
            &mut StimulationNeed,
            &mut EnvironmentNeed,
            &mut HealthNeed,
            &mut HygieneNeed,
            &mut BathroomNeed,
            &mut NeedTriggerState,
        ),
        (With<Animal>, Without<Dead>),
    >,
    mut changed: MessageWriter<NeedChanged>,
) {
    for message in messages.read() {
        // Setting reproduction is the adjustment from its current value.
        if message.need == NeedKind::Reproduction {
            if let Ok(current) = reproduction_needs.get(message.animal) {
                reproduction.write(AdjustReproduction {
                    animal: message.animal,
                    delta_q16: message.value_q16.clamp(0, MAX_NEED_Q16) - current.0,
                });
            }
            continue;
        }
        let Ok((
            species,
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
            mut triggers,
        )) = animals.get_mut(message.animal)
        else {
            continue;
        };
        let Some(needs) = species_needs(&index, &assets, species) else {
            continue;
        };
        let (index, value) = match message.need {
            NeedKind::Hunger => (0, &mut hunger.0),
            NeedKind::Thirst => (1, &mut thirst.0),
            NeedKind::Rest => (2, &mut rest.0),
            NeedKind::Privacy => (3, &mut privacy.0),
            NeedKind::Social => (4, &mut social.0),
            NeedKind::Exercise => (5, &mut exercise.0),
            NeedKind::Stimulation => (6, &mut stimulation.0),
            NeedKind::Environment => (7, &mut environment.0),
            NeedKind::Health => (8, &mut health.0),
            NeedKind::Hygiene => (9, &mut hygiene.0),
            NeedKind::Bathroom => (10, &mut bathroom.0),
            NeedKind::Reproduction => unreachable!("handled above"),
        };
        let old = *value;
        let next = message.value_q16.clamp(0, MAX_NEED_Q16);
        *value = next;
        triggers.observe(
            index,
            next,
            needs[index].trigger_wellness_threshold,
            needs[index].cessation_wellness_threshold,
        );
        if animal_need_crossed_authored_threshold(
            old,
            next,
            needs[index].trigger_wellness_threshold,
            needs[index].cessation_wellness_threshold,
        ) {
            changed.write(NeedChanged {
                animal: message.animal,
                need: message.need,
                value: convert_animal_need_q16_to_permille(next),
            });
        }
    }
}

pub(super) fn derive_animal_welfare(
    mut animals: Query<
        (Entity, &NeedPoints, &mut AnimalWelfare, &mut WelfareBand),
        (With<Animal>, Without<Dead>, Changed<NeedPoints>),
    >,
    mut changed: MessageWriter<WelfareChanged>,
) {
    for (entity, points, mut welfare, mut band) in &mut animals {
        let (next_value, next_band) = welfare_from_need_points(points);
        if welfare.0 != next_value {
            welfare.0 = next_value;
        }
        if *band != next_band {
            changed.write(WelfareChanged {
                animal: entity,
                old: *band,
                new: next_band,
            });
            *band = next_band;
        }
    }
}
