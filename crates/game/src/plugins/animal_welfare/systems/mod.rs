use bevy::prelude::*;
use openzt2_game_data::species::{NeedKind, SpeciesNeed};

use crate::{
    assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    plugins::{
        animal_health::types::Dead,
        animal_lifecycle::types::{Animal, SpeciesHandle},
        habitat::habitat_types::{HabitatChanged, HabitatMember, HabitatSummary},
        simulation_time::deterministic_random_stream::{DeterministicRng, RngDomain, ZooSeed},
        world_spawn::persistent_id_types::PersistentId,
    },
};

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

pub(super) fn initialize_animal_needs(
    mut commands: Commands,
    zoo_seed: Res<ZooSeed>,
    assets: Res<Assets<SpeciesAsset>>,
    index: Res<SpeciesAssets>,
    animals: Query<
        (
            Entity,
            &PersistentId,
            &SpeciesHandle,
            Option<&HabitatMember>,
        ),
        (With<Animal>, Without<AnimalWelfare>, Without<Dead>),
    >,
    habitats: Query<&HabitatSummary>,
) {
    for (entity, persistent_id, species, member) in &animals {
        let Some(needs) = species_needs(&index, &assets, species) else {
            continue;
        };
        let mut random =
            DeterministicRng::from_entity(*zoo_seed, *persistent_id, RngDomain::AnimalNeeds);
        let initial = needs.map(|need| {
            let minimum = need.initial_wellness_permille_range[0];
            let maximum = need.initial_wellness_permille_range[1].max(minimum);
            let width = u32::from(maximum - minimum) + 1;
            let sampled = minimum + random.range_u32(width).unwrap_or_default() as u16;
            i32::from(sampled) * Q16_ONE
        });
        let suitability = member
            .and_then(|member| habitats.get(member.habitat_entity).ok())
            .and_then(|summary| index.get(&assets).map(|_| suitability(summary, needs)))
            .unwrap_or_default();
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

macro_rules! decay_system {
    ($function:ident, $component:ty, $index:expr, $kind:expr) => {
        pub(super) fn $function(
            assets: Res<Assets<SpeciesAsset>>,
            index: Res<SpeciesAssets>,
            mut animals: Query<
                (
                    Entity,
                    &SpeciesHandle,
                    &mut $component,
                    &mut NeedTriggerState,
                ),
                (With<Animal>, Without<Dead>),
            >,
            mut changed: MessageWriter<NeedChanged>,
        ) {
            for (entity, species, mut value, mut triggers) in &mut animals {
                let Some(needs) = species_needs(&index, &assets, species) else {
                    continue;
                };
                let (next, crossed) = rate_result(value.0, needs[$index]);
                if next != value.0 {
                    value.0 = next;
                }
                triggers.observe(
                    $index,
                    next,
                    needs[$index].trigger_wellness_threshold,
                    needs[$index].cessation_wellness_threshold,
                );
                if crossed {
                    changed.write(NeedChanged {
                        animal: entity,
                        need: $kind,
                        value: convert_animal_need_q16_to_permille(next),
                    });
                }
            }
        }
    };
}

decay_system!(decay_hunger, Hunger, 0, NeedKind::Hunger);
decay_system!(decay_thirst, Thirst, 1, NeedKind::Thirst);
decay_system!(decay_rest, RestNeed, 2, NeedKind::Rest);
decay_system!(decay_privacy, PrivacyNeed, 3, NeedKind::Privacy);
decay_system!(decay_social, SocialNeed, 4, NeedKind::Social);
decay_system!(decay_exercise, ExerciseNeed, 5, NeedKind::Exercise);
decay_system!(decay_stimulation, StimulationNeed, 6, NeedKind::Stimulation);
decay_system!(decay_environment, EnvironmentNeed, 7, NeedKind::Environment);
decay_system!(decay_health_need, HealthNeed, 8, NeedKind::Health);
decay_system!(decay_hygiene, HygieneNeed, 9, NeedKind::Hygiene);
decay_system!(decay_bathroom, BathroomNeed, 10, NeedKind::Bathroom);

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

/// Weight the habitat biome areas by the species preferences.
fn resolve_biome_map_score(summary: &HabitatSummary, environment: &SpeciesNeed) -> u16 {
    let total = summary.land_area_square_metres + summary.water_area_square_metres;
    if total <= 0.0 || !total.is_finite() {
        return 0;
    }
    let preferences = &environment.preferences;
    if preferences.is_empty() {
        return 1000;
    }
    let mut weighted = 0.0_f64;
    let mut weights = 0.0_f64;
    for preference in preferences {
        let weight = f64::from(preference.weight.max(0));
        let area = summary
            .biome_areas_square_metres
            .iter()
            .find(|(id, _)| id.0 == preference.target.0)
            .map_or(0.0, |(_, area)| *area);
        let quality = ((area / total).clamp(0.0, 1.0) * 1000.0).round() as u16;
        let accepted = if quality >= preference.minimum_quality {
            quality
        } else {
            0
        };
        weighted += f64::from(accepted) * weight;
        weights += weight;
    }
    if weights == 0.0 {
        1000
    } else {
        (weighted / weights).round().clamp(0.0, 1000.0) as u16
    }
}

fn suitability(summary: &HabitatSummary, needs: [&SpeciesNeed; 11]) -> HabitatSuitability {
    let has_space = !summary.boundary_is_breached
        && summary.land_area_square_metres + summary.water_area_square_metres > 0.0;
    let space = if has_space { 1000 } else { 0 };
    let biome = resolve_biome_map_score(summary, needs[7]);
    HabitatSuitability {
        space,
        biome,
        overall: space.min(biome),
    }
}

pub(super) fn assess_habitat_suitability(
    assets: Res<Assets<SpeciesAsset>>,
    index: Res<SpeciesAssets>,
    mut events: MessageReader<HabitatChanged>,
    mut animals: Query<
        (&SpeciesHandle, &HabitatMember, &mut HabitatSuitability),
        (With<Animal>, Without<Dead>),
    >,
    habitats: Query<&HabitatSummary>,
) {
    for event in events.read() {
        let Ok(summary) = habitats.get(event.habitat_entity) else {
            continue;
        };
        for (species, member, mut current) in &mut animals {
            if member.habitat_entity != event.habitat_entity {
                continue;
            }
            let Some(needs) = species_needs(&index, &assets, species) else {
                continue;
            };
            let next = suitability(summary, needs);
            if *current != next {
                *current = next;
            }
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
        let Some(needs) = species_needs(&index, &assets, species) else {
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
        if welfare.0 != next_value {
            welfare.0 = next_value;
        }
    }
}
