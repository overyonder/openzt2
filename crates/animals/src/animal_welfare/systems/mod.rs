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
        animal_need_crossed_authored_threshold, calculate_mean_animal_welfare_with_habitat_limit,
        clamp_animal_need_q16_after_delta, classify_animal_welfare_against_authored_thresholds,
        convert_animal_need_q16_to_permille,
    },
    types::*,
};

fn need_rank(kind: &NeedKind) -> usize {
    match kind {
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
    }
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
        found[need_rank(&row.kind)] = Some(row);
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
        let mut initial = needs.map(|need| {
            let minimum = need.initial_wellness_permille_range[0];
            let maximum = need.initial_wellness_permille_range[1].max(minimum);
            let width = u32::from(maximum - minimum) + 1;
            let sampled = minimum + random.range_u32(width).unwrap_or_default() as u16;
            i32::from(sampled) * Q16_ONE
        });
        if let Some(scripted) = scripted {
            for (kind, value) in &scripted.needs {
                initial[need_rank(kind)] = (*value).clamp(0, MAX_NEED_Q16);
            }
        }
        // The next assessment uses live membership and species population.
        let suitability = HabitatSuitability::default();
        let value = calculate_mean_animal_welfare_with_habitat_limit(
            std::array::from_fn(|index| initial[index]),
            suitability,
        );
        let critical = needs
            .iter()
            .take(10)
            .map(|need| need.critical_wellness_threshold.unwrap_or(0))
            .sum::<u16>()
            / 10;
        let good = needs
            .iter()
            .take(10)
            .map(|need| need.cessation_wellness_threshold.unwrap_or(1000))
            .sum::<u16>()
            / 10;
        let mut trigger_state = NeedTriggerState::default();
        for (index, need) in needs.iter().enumerate() {
            trigger_state.observe(
                index,
                initial[index],
                need.trigger_wellness_threshold,
                need.cessation_wellness_threshold,
            );
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
            classify_animal_welfare_against_authored_thresholds(value, critical, good),
        ));
        if scripted.is_some() {
            commands.entity(entity).remove::<crate::animal_lifecycle::scripted_animal_spawning::ScriptedAnimalInitialization>();
        }
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
    assets: Res<Assets<SpeciesAsset>>,
    index: Res<SpeciesAssets>,
    needs: Query<
        (
            Entity,
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
            &HabitatSuitability,
        ),
        Or<(
            Changed<Hunger>,
            Changed<Thirst>,
            Changed<RestNeed>,
            Changed<PrivacyNeed>,
            Changed<SocialNeed>,
            Changed<ExerciseNeed>,
            Changed<StimulationNeed>,
            Changed<EnvironmentNeed>,
            Changed<HealthNeed>,
            Changed<HygieneNeed>,
            Changed<HabitatSuitability>,
        )>,
    >,
    mut animals: Query<
        (&SpeciesHandle, &mut AnimalWelfare, &mut WelfareBand),
        (With<Animal>, Without<Dead>),
    >,
    mut changed: MessageWriter<WelfareChanged>,
) {
    for (
        entity,
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
        habitat,
    ) in &needs
    {
        let Ok((species, mut welfare, mut band)) = animals.get_mut(entity) else {
            continue;
        };
        let values = [
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
        ];
        let next_value = calculate_mean_animal_welfare_with_habitat_limit(values, *habitat);
        if welfare.0 != next_value {
            welfare.0 = next_value;
        }
        let Some(needs) = species_needs(&index, &assets, species) else {
            continue;
        };
        let critical = needs
            .iter()
            .take(10)
            .map(|need| need.critical_wellness_threshold.unwrap_or(0))
            .sum::<u16>()
            / 10;
        let good = needs
            .iter()
            .take(10)
            .map(|need| need.cessation_wellness_threshold.unwrap_or(1000))
            .sum::<u16>()
            / 10;
        let next_band =
            classify_animal_welfare_against_authored_thresholds(next_value, critical, good);
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
