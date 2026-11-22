use bevy::prelude::*;
use openzt2_game_data::species::{AnimalNeedPointPolicy, NeedKind, SpeciesNeed};

use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_world_state::simulation_time::deterministic_random_stream::RngDomain;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::animal_lifecycle::types::{Animal, AnimalVariant};

use super::{
    calculations::{
        calculate_mean_animal_welfare_with_habitat_limit,
        classify_animal_welfare_against_authored_thresholds,
    },
    types::*,
};

pub(super) fn need_rank(kind: &NeedKind) -> Option<usize> {
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
    zoo_seed: Single<&ZooSeed>,
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
            DeterministicRng::from_entity(**zoo_seed, *persistent_id, RngDomain::AnimalNeeds);
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
            classify_animal_welfare_against_authored_thresholds(value, critical, good),
        ));
        if scripted.is_some() {
            commands.entity(entity).remove::<crate::animal_lifecycle::scripted_animal_spawning::ScriptedAnimalInitialization>();
        }
    }
}

#[derive(Component, PartialEq)]
#[require(ActiveNeedPointBranch)]
pub(super) struct AuthoredNeedPointPolicy(AnimalNeedPointPolicy);

#[derive(Component, Default)]
pub(super) struct ActiveNeedPointBranch {
    unmet: Option<bool>,
    factor: f32,
}

pub(super) fn project_need_point_policy(
    assets: Res<Assets<SpeciesAsset>>,
    index: Res<SpeciesAssets>,
    animals: Query<
        (
            Entity,
            Ref<AnimalVariant>,
            Option<&mut AuthoredNeedPointPolicy>,
        ),
        With<Animal>,
    >,
    mut commands: Commands,
) {
    let all = assets.is_changed() || index.is_changed();
    let Some(species) = index.get(&assets) else {
        return;
    };
    let mut animals = animals;
    for (animal, variant, current) in &mut animals {
        if !all && !variant.is_changed() {
            continue;
        }
        let Some(variant) = species.find_variant(variant.0) else {
            if current.is_some() {
                commands.entity(animal).remove::<AuthoredNeedPointPolicy>();
            }
            continue;
        };
        let policy = AuthoredNeedPointPolicy(variant.need_points);
        if let Some(mut current) = current {
            current.set_if_neq(policy);
        } else {
            commands.entity(animal).insert(policy);
        }
    }
}

pub(super) fn step_need_points(
    points: &mut NeedPoints,
    branch: &mut ActiveNeedPointBranch,
    policy: &AnimalNeedPointPolicy,
    unmet: bool,
) -> (f32, f32) {
    if branch.unmet != Some(unmet) {
        branch.unmet = Some(unmet);
        branch.factor = policy.factor;
        if unmet {
            points.good = 0.0;
        } else {
            points.bad = 0.0;
        }
    } else {
        branch.factor += policy.factor_delta;
    }
    if unmet {
        let delta = policy.bad_delta * branch.factor;
        points.bad = (points.bad + delta).min(policy.maximum);
        (0.0, delta)
    } else {
        let delta = policy.good_delta * branch.factor;
        points.good = (points.good + delta).min(policy.maximum);
        (delta, 0.0)
    }
}

pub(super) fn update_need_points(
    time: Res<Time<Fixed>>,
    mut animals: Query<
        (
            Entity,
            &NeedTriggerState,
            Ref<AuthoredNeedPointPolicy>,
            &mut ActiveNeedPointBranch,
            &mut NeedPoints,
        ),
        (With<Animal>, Without<Dead>),
    >,
    mut outcomes: MessageWriter<AccumulateNeedPoints>,
) {
    for (animal, triggers, policy, mut branch, mut points) in &mut animals {
        let unmet = triggers.0 != 0;
        let remaining = points.seconds_until_update - time.delta_secs();
        if remaining > 0.0 && branch.unmet == Some(unmet) && !policy.is_changed() {
            points.seconds_until_update = remaining;
            continue;
        }
        if policy.is_changed() {
            branch.unmet = None;
        }
        points.seconds_until_update = policy.0.interval_seconds;
        let (good, bad) = step_need_points(&mut points, &mut branch, &policy.0, unmet);
        if good != 0.0 || bad != 0.0 {
            outcomes.write(AccumulateNeedPoints { animal, good, bad });
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

/// Animals from saves made before the reproduction need start it from its
/// authored initial range.
pub(crate) fn initialize_missing_reproduction_needs(
    mut commands: Commands,
    zoo_seed: Single<&ZooSeed>,
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
            DeterministicRng::from_entity(**zoo_seed, *persistent_id, RngDomain::AnimalNeeds);
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

pub(super) fn derive_animal_welfare(
    profiles: Res<super::need_updates::SpeciesNeedProfiles>,
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
        let Some(profile) = profiles.rows.get(&species.species) else {
            continue;
        };
        let next_band = classify_animal_welfare_against_authored_thresholds(
            next_value,
            profile.critical,
            profile.good,
        );
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
