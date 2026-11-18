//! `zt.exe`'s rampage manager (`BFGRampageMgr`, `config/rampage.xml`): every
//! 3.5 s an unhappy animal that can rampage may start one, more likely the
//! more bad need points it has. A rampage lasts about five minutes, and the
//! animal can't start another for five more.

use bevy::prelude::*;

use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_game_data::species::SpeciesVariantFlags;
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_zoo_state::animal_behavior::behavior_random_stream_state::BehaviorRandomStream;
use openzt2_zoo_state::animal_lifecycle::types::{Animal, AnimalVariant};
use openzt2_zoo_state::animal_welfare::types::NeedPoints;

use super::types::{Dead, Rampaging, Tranquilized};

// `config/rampage.xml`.
const UPDATE_SECONDS: f32 = 3.5;
const MAX_GOOD_POINTS: f32 = 10.0;
const MAX_BAD_POINTS: f32 = 60.0;
const BASE_CHANCE_PERCENT: f32 = 0.2;
const MAX_CHANCE_PERCENT: f32 = 4.0;
const REPEAT_RAMPAGER_FACTOR: f32 = 1.2;
const RAMPAGE_SECONDS: f32 = 300.0;
const RAMPAGE_VARIANCE_SECONDS: f32 = 30.0;
const RAMPAGE_INTERVAL_SECONDS: f32 = 300.0;

/// One animal's rampage timers.
#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct RampageSchedule {
    seconds_until_check: f32,
    /// Left of the current rampage, or of the rest after one.
    seconds_remaining: f32,
    rampaged_before: bool,
}

/// The chance, in percent, that an animal starts a rampage at one check.
///
/// `rampage.xml` names a base and a maximum chance, and the good and bad need
/// points at which they apply. Read that way, bad points up to
/// `maxBadPoints` raise the chance from the base towards the maximum, and
/// good points up to `maxGoodPoints` take that rise away again: an animal
/// with 10 good points is content whatever its bad ones. An animal with no
/// bad points doesn't rampage. `rampagingAnimalFactor` (1.2) is read as
/// making a repeat rampager more likely to go again, still capped at the
/// maximum.
pub(crate) fn rampage_chance_percent(points: &NeedPoints, rampaged_before: bool) -> f32 {
    let pressure = (points.bad / MAX_BAD_POINTS).clamp(0.0, 1.0)
        * (1.0 - (points.good / MAX_GOOD_POINTS).clamp(0.0, 1.0));
    if pressure <= 0.0 {
        return 0.0;
    }
    let chance = BASE_CHANCE_PERCENT + (MAX_CHANCE_PERCENT - BASE_CHANCE_PERCENT) * pressure;
    if rampaged_before {
        (chance * REPEAT_RAMPAGER_FACTOR).min(MAX_CHANCE_PERCENT)
    } else {
        chance
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn evaluate_animal_rampage_start_and_end_conditions(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    (species_assets, species_index): (Res<Assets<SpeciesAsset>>, Res<SpeciesAssets>),
    mut animals: Query<
        (
            Entity,
            &AnimalVariant,
            &NeedPoints,
            Option<&mut BehaviorRandomStream>,
            Option<&mut RampageSchedule>,
            Has<Rampaging>,
            Has<Tranquilized>,
            Has<WorldEntityIsCrated>,
        ),
        (With<Animal>, Without<Dead>),
    >,
) {
    let Some(species) = species_index.get(&species_assets) else {
        return;
    };
    let delta = time.delta_secs();
    for (animal, variant, points, random, schedule, rampaging, tranquilized, crated) in &mut animals
    {
        if !species
            .find_variant(variant.0)
            .is_some_and(|variant| variant.flags.contains(SpeciesVariantFlags::RAMPAGER))
        {
            continue;
        }
        let Some(mut schedule) = schedule else {
            commands.entity(animal).insert(RampageSchedule {
                seconds_until_check: UPDATE_SECONDS,
                ..default()
            });
            continue;
        };
        if !tranquilized {
            schedule.seconds_remaining = (schedule.seconds_remaining - delta).max(0.0);
        }
        if rampaging {
            if schedule.seconds_remaining <= 0.0 {
                debug!(?animal, "animal rampage ended");
                commands.entity(animal).remove::<Rampaging>();
                schedule.seconds_remaining = RAMPAGE_INTERVAL_SECONDS;
            }
            continue;
        }
        schedule.seconds_until_check -= delta;
        if schedule.seconds_until_check > 0.0 {
            continue;
        }
        schedule.seconds_until_check += UPDATE_SECONDS;
        let Some(mut random) = random else {
            continue;
        };
        if schedule.seconds_remaining > 0.0 || tranquilized || crated {
            continue;
        }
        let chance = rampage_chance_percent(points, schedule.rampaged_before);
        // A sample in hundredths of a percent.
        if (random.next_u32() % 10_000) as f32 >= chance * 100.0 {
            continue;
        }
        let variance = (random.next_u32() % 10_001) as f32 / 10_000.0 * 2.0 - 1.0;
        schedule.seconds_remaining = RAMPAGE_SECONDS + RAMPAGE_VARIANCE_SECONDS * variance;
        schedule.rampaged_before = true;
        debug!(
            ?animal,
            chance,
            seconds = schedule.seconds_remaining,
            "animal rampage started"
        );
        commands
            .entity(animal)
            .insert(Rampaging { elapsed_ticks: 0 });
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
    fn an_unhappy_animal_is_likelier_to_rampage_and_a_contented_one_never_does() {
        assert_eq!(rampage_chance_percent(&points(0.0, 0.0), false), 0.0);
        assert_eq!(rampage_chance_percent(&points(10.0, 60.0), false), 0.0);
        assert!((rampage_chance_percent(&points(0.0, 30.0), false) - 2.1).abs() < 1e-4);
        assert!((rampage_chance_percent(&points(0.0, 60.0), false) - 4.0).abs() < 1e-4);
        assert!((rampage_chance_percent(&points(0.0, 30.0), true) - 2.52).abs() < 1e-4);
        assert_eq!(rampage_chance_percent(&points(0.0, 90.0), true), 4.0);
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
