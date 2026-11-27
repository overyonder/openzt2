use bevy::prelude::*;
use openzt2_game_data::species::{AnimalNeedPointPolicy, NeedKind, SpeciesNeed};

use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets, SpeciesView};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_world_state::simulation_time::deterministic_random_stream::RngDomain;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooTimekeeperElapsed;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::animal_lifecycle::types::{Animal, AnimalVariant};

use super::{
    calculations::{
        calculate_mean_animal_welfare_with_habitat_limit,
        classify_animal_welfare_against_authored_thresholds,
    },
    need_latch_types::{AnimalNeedLatchBundle, AnimalNeedLatches},
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
    view: SpeciesView<'a>,
    species: openzt2_game_data::AssetId,
) -> Option<&'a SpeciesNeed> {
    view.find(species)?
        .needs
        .iter()
        .find(|need| need.kind == NeedKind::Reproduction)
}

fn species_needs<'a>(
    view: SpeciesView<'a>,
    species: openzt2_game_data::AssetId,
) -> Option<[&'a SpeciesNeed; 11]> {
    let species = view.find(species)?;
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
    seeds: Query<&ZooSeed>,
    assets: Res<Assets<SpeciesAsset>>,
    index: Res<SpeciesAssets>,
    animals: Query<
        (
            Entity,
            &PersistentId,
            &WorldMember,
            &SpeciesHandle,
            Option<
                &crate::animal_lifecycle::scripted_animal_spawning::ScriptedAnimalInitialization,
            >,
        ),
        (With<Animal>, Without<AnimalWelfare>, Without<Dead>),
    >,
) {
    let Some(view) = index.get(&assets) else {
        return;
    };
    for (entity, persistent_id, member, species, scripted) in &animals {
        let Ok(seed) = seeds.get(member.root) else {
            continue;
        };
        let initialized = initialize_one_animal_needs(
            &mut commands.entity(entity),
            *seed,
            *persistent_id,
            view,
            species.species,
            scripted.map_or(&[], |scripted| scripted.needs.as_slice()),
        );
        if initialized && scripted.is_some() {
            commands.entity(entity).remove::<crate::animal_lifecycle::scripted_animal_spawning::ScriptedAnimalInitialization>();
        }
    }
}

pub(crate) fn initialize_one_animal_needs(
    entity: &mut EntityCommands,
    seed: ZooSeed,
    persistent_id: PersistentId,
    view: SpeciesView<'_>,
    species: openzt2_game_data::AssetId,
    initial_needs: &[(NeedKind, i32)],
) -> bool {
    let Some(needs) = species_needs(view, species) else {
        return false;
    };
    let mut random = DeterministicRng::from_entity(seed, persistent_id, RngDomain::AnimalNeeds);
    let mut sample = |need: &SpeciesNeed| {
        let minimum = need.initial_wellness_permille_range[0];
        let maximum = need.initial_wellness_permille_range[1].max(minimum);
        let width = u32::from(maximum - minimum) + 1;
        let sampled = minimum + random.range_u32(width).unwrap_or_default() as u16;
        i32::from(sampled) * Q16_ONE
    };
    let mut initial = needs.map(&mut sample);
    // Sampled after the welfare needs so their streams stay as they were.
    let reproduction_row = species_reproduction_need(view, species);
    let mut reproduction = reproduction_row.map(&mut sample);
    {
        for (kind, value) in initial_needs {
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
    let mut trigger_mask = 0;
    for (index, need) in needs.iter().enumerate() {
        if need
            .trigger_wellness_threshold
            .is_some_and(|threshold| initial[index] <= i32::from(threshold) * Q16_ONE)
        {
            trigger_mask |= 1 << index;
        }
    }
    if let (Some(row), Some(value)) = (reproduction_row, reproduction) {
        if row
            .trigger_wellness_threshold
            .is_some_and(|threshold| value <= i32::from(threshold) * Q16_ONE)
        {
            trigger_mask |= 1 << REPRODUCTION_TRIGGER_INDEX;
        }
        entity.insert(ReproductionNeed(value));
    }
    entity.insert((
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
    entity.insert((
        HygieneNeed(initial[9]),
        BathroomNeed(initial[10]),
        AnimalNeedLatchBundle::from_saved_mask(trigger_mask),
        suitability,
        AnimalWelfare(value),
        CumulativeNeedPoints::default(),
        NeedPoints::default(),
        classify_animal_welfare_against_authored_thresholds(value, critical, good),
    ));
    true
}

#[derive(Component, PartialEq)]
#[require(ActiveNeedPointBranch, NeedPoints)]
pub(super) struct AuthoredNeedPointPolicy(AnimalNeedPointPolicy);

#[derive(Component, Default)]
pub(super) struct ActiveNeedPointBranch {
    unmet: Option<bool>,
    factor: f32,
    cadence: Timer,
}

pub(super) fn project_need_point_policy(
    assets: Res<Assets<SpeciesAsset>>,
    index: Res<SpeciesAssets>,
    mut events: MessageReader<AssetEvent<SpeciesAsset>>,
    mut animals: ParamSet<(
        Query<(Entity, &AnimalVariant, Option<&mut AuthoredNeedPointPolicy>), With<Animal>>,
        Query<
            (Entity, &AnimalVariant, Option<&mut AuthoredNeedPointPolicy>),
            (With<Animal>, Or<(Added<Animal>, Changed<AnimalVariant>)>),
        >,
    )>,
    mut commands: Commands,
) {
    let all = index.is_changed()
        || events.read().any(|event| {
            matches!(
                event,
                AssetEvent::Added { .. } | AssetEvent::Modified { .. } | AssetEvent::Removed { .. }
            )
        });
    events.clear();
    let Some(species) = index.get(&assets) else {
        return;
    };
    let mut project = |(animal, variant, current): (
        Entity,
        &AnimalVariant,
        Option<Mut<AuthoredNeedPointPolicy>>,
    )| {
        let Some(variant) = species.find_variant(variant.0) else {
            if current.is_some() {
                commands.entity(animal).remove::<AuthoredNeedPointPolicy>();
            }
            return;
        };
        let policy = AuthoredNeedPointPolicy(variant.need_points);
        if let Some(mut current) = current {
            current.set_if_neq(policy);
        } else {
            commands.entity(animal).insert(policy);
        }
    };
    if all {
        animals.p0().iter_mut().for_each(&mut project);
    } else {
        animals.p1().iter_mut().for_each(project);
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
    mut elapsed: MessageReader<ZooTimekeeperElapsed>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut animals: Query<
        (
            Entity,
            &WorldMember,
            AnimalNeedLatches,
            Ref<AuthoredNeedPointPolicy>,
            &mut ActiveNeedPointBranch,
            &mut NeedPoints,
        ),
        (With<Animal>, Without<Dead>),
    >,
    mut outcomes: MessageWriter<AccumulateNeedPoints>,
) {
    let step = definitions
        .get(&assets)
        .and_then(|view| view.authored_timekeeper_policy())
        .map_or_else(|| time.timestep(), |policy| policy.slow.interval);
    let mut counts = bevy::ecs::entity::EntityHashMap::<u32>::default();
    for tick in elapsed.read().filter(|tick| {
        tick.event == openzt2_game_assets::world_definitions::timing::TimekeeperEvent::Slow
    }) {
        let count = counts.entry(tick.root).or_default();
        *count = count.saturating_add(tick.count);
    }
    for (animal, member, triggers, policy, mut branch, mut points) in &mut animals {
        let Some(&count) = counts.get(&member.root) else {
            continue;
        };
        let unmet = triggers.any_triggered();
        if policy.is_changed() {
            branch.unmet = None;
        }
        for _ in 0..count {
            let due = branch.cadence.tick(step).is_finished();
            if !due && branch.unmet == Some(unmet) {
                continue;
            }
            branch.cadence.set_duration(
                std::time::Duration::try_from_secs_f32(policy.0.interval_seconds)
                    .unwrap_or_default(),
            );
            branch.cadence.reset();
            let (good, bad) = step_need_points(&mut points, &mut branch, &policy.0, unmet);
            if good != 0.0 || bad != 0.0 {
                outcomes.write(AccumulateNeedPoints { animal, good, bad });
            }
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
    seeds: Query<&ZooSeed>,
    assets: Res<Assets<SpeciesAsset>>,
    index: Res<SpeciesAssets>,
    animals: Query<
        (Entity, &PersistentId, &WorldMember, &SpeciesHandle),
        (
            With<Animal>,
            With<AnimalWelfare>,
            Without<ReproductionNeed>,
            Without<Dead>,
        ),
    >,
) {
    for (entity, persistent_id, member, species) in &animals {
        let Ok(seed) = seeds.get(member.root) else {
            continue;
        };
        let Some(need) = index
            .get(&assets)
            .and_then(|view| species_reproduction_need(view, species.species))
        else {
            continue;
        };
        let mut random =
            DeterministicRng::from_entity(*seed, *persistent_id, RngDomain::AnimalNeeds);
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
