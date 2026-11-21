use bevy::prelude::*;

use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_game_data::species::SpeciesVariantFlags;
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_zoo_state::animal_behavior::behavior_random_stream_state::BehaviorRandomStream;
use openzt2_zoo_state::animal_lifecycle::types::{Animal, AnimalVariant};
use openzt2_zoo_state::animal_welfare::types::NeedPoints;

use super::rampage_policy::{RampagePolicy, RampagePolicySource};
use super::types::{Dead, Rampaging, Tranquilized};

#[derive(Component)]
struct RampageStartCheck(f32);

#[derive(Component)]
struct RampageTimeRemaining(f32);

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
    );
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
    mut policy_ready: Local<bool>,
    mut commands: Commands,
) {
    let (Some(policy), Some(species)) = (policy.get(), index.get(&assets)) else {
        *policy_ready = false;
        return;
    };
    let definitions_changed = !*policy_ready || assets.is_changed() || index.is_changed();
    *policy_ready = true;
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
            commands
                .entity(animal)
                .insert(RampageStartCheck(policy.check_interval));
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
        cooldown.0 = (cooldown.0 - time.delta_secs()).max(0.0);
        if cooldown.0 == 0.0 {
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
            Without<WorldEntityIsCrated>,
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
        check.0 -= time.delta_secs();
        if check.0 > 0.0 {
            continue;
        }
        check.0 += policy.check_interval;
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
            Rampaging { elapsed_ticks: 0 },
            RampageTimeRemaining(policy.duration + policy.variance * variance),
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
            timer.0 = (timer.0 - time.delta_secs()).max(0.0);
            if timer.0 > 0.0 {
                continue;
            }
            timer.0 = policy.cooldown;
            commands.entity(animal).remove::<Rampaging>();
        } else {
            commands
                .entity(animal)
                .remove::<Rampaging>()
                .insert(RampageTimeRemaining(policy.cooldown));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points(good: f32, bad: f32) -> NeedPoints {
        NeedPoints {
            good,
            bad,
            seconds_until_update: 0.0,
        }
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

pub(super) fn advance_elapsed_ticks_for_active_untranquilized_animal_rampages(
    mut rampaging_animals: Query<
        &mut Rampaging,
        (With<Animal>, Without<Dead>, Without<Tranquilized>),
    >,
) {
    for mut rampage in &mut rampaging_animals {
        rampage.elapsed_ticks = rampage.elapsed_ticks.saturating_add(1);
    }
}
