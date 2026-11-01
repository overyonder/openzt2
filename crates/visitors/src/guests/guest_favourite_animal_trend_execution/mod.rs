use bevy::prelude::*;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use std::collections::BTreeSet;

use openzt2_game_data::{
    AssetId,
    world_definitions::{
        catalogue_and_progression::animal_adoption_species_eligibility::AnimalAdoptionSpeciesEligibility,
        guest_favourite_animal_trend_policy::GuestFavouriteAnimalTrendPolicy,
    },
};
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted, world_membership_types::WorldRoot,
};
use openzt2_zoo_state::guests::guest_favourite_animal_trend_types::GuestFavouriteAnimalTrend;
use openzt2_zoo_state::guests::guest_simulation_types::GuestArrivalState;

fn create_guest_favourite_animal_trend<'a>(
    candidates: impl Iterator<Item = &'a AnimalAdoptionSpeciesEligibility>,
    random: &mut DeterministicRng,
) -> GuestFavouriteAnimalTrend {
    let species = candidates
        .filter(|candidate| candidate.rarity_fame_percent <= 100.0)
        .map(|candidate| candidate.species)
        .collect();
    GuestFavouriteAnimalTrend::new(choose_species(&species, random))
}

pub(super) fn initialize_loaded_world_guest_favourite_animal_trends(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    arrivals: Option<ResMut<GuestArrivalState>>,
    mut failed: MessageWriter<
        openzt2_zoo_state::guests::guest_simulation_types::GuestArrivalFailed,
    >,
    roots: Query<
        Entity,
        (
            With<WorldRoot>,
            With<WorldLoadCompleted>,
            Without<GuestFavouriteAnimalTrend>,
        ),
    >,
) {
    let (Some(mut arrivals), Some(definitions)) = (arrivals, active.get(&definitions)) else {
        return;
    };
    if definitions.has_pending_assets() {
        return;
    }
    if definitions.guest_favourite_animal_trend().is_none() {
        failed.write(openzt2_zoo_state::guests::guest_simulation_types::GuestArrivalFailed {
            reason: openzt2_zoo_state::guests::guest_simulation_types::GuestArrivalFailure::MissingPolicy,
        });
        return;
    }
    for root in &roots {
        let trend = create_guest_favourite_animal_trend(
            definitions.animal_adoption_species_eligibility(),
            &mut arrivals.rng,
        );
        commands.entity(root).insert(trend);
    }
}

pub(super) fn choose_favourite_animal_for_spawned_guest<'a>(
    policy: &GuestFavouriteAnimalTrendPolicy,
    trend: &mut GuestFavouriteAnimalTrend,
    candidates: impl Iterator<Item = &'a AnimalAdoptionSpeciesEligibility>,
    current_fame_percent: f32,
    random: &mut DeterministicRng,
) -> Option<AssetId> {
    let mut all_species = BTreeSet::new();
    let mut fame_species = BTreeSet::new();
    for candidate in candidates {
        if candidate.rarity_fame_percent <= 100.0 {
            all_species.insert(candidate.species);
        }
        if candidate.rarity_fame_percent <= current_fame_percent {
            fame_species.insert(candidate.species);
        }
    }
    let favourite = if i64::from(random.next_u32() % 100) < i64::from(policy.trend_group_percent) {
        let trending =
            if i64::from(random.next_u32() % 100) < i64::from(policy.trend_setter_percent) {
                trend
                    .species()
                    .into_iter()
                    .flatten()
                    .collect::<BTreeSet<_>>()
            } else {
                BTreeSet::new()
            };
        if trending.is_empty() {
            choose_species(&fame_species, random)
        } else {
            choose_species(&trending, random)
        }
    } else {
        choose_species(&all_species, random)
    };
    trend.advance_after_guest_spawn(policy, |excluded| {
        if let Some(excluded) = excluded {
            all_species.remove(&excluded);
        }
        choose_species(&all_species, random)
    });
    favourite
}

fn choose_species(species: &BTreeSet<AssetId>, random: &mut DeterministicRng) -> Option<AssetId> {
    if species.is_empty() {
        return None;
    }
    species
        .iter()
        .nth(random.next_u32() as usize % species.len())
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_fame_boundary_and_saved_rollover_preserve_species_identity() {
        let first = AssetId::from_key("zebra");
        let next = AssetId::from_key("jaguar");
        let candidates = [
            AnimalAdoptionSpeciesEligibility {
                id: AssetId::from_key("jaguaradult"),
                species: next,
                rarity_fame_percent: 2.0,
            },
            AnimalAdoptionSpeciesEligibility {
                id: AssetId::from_key("jaguaryoung"),
                species: next,
                rarity_fame_percent: 2.0,
            },
        ];
        let policy = GuestFavouriteAnimalTrendPolicy {
            trend_group_percent: 100,
            trend_setter_percent: 0,
            spawn_thresholds: [40, 10],
        };
        let mut trend =
            GuestFavouriteAnimalTrend::from_saved_state([Some(first), None], 38, 0).unwrap();
        let mut random = DeterministicRng::from_raw([0, 1]);
        assert_eq!(
            choose_favourite_animal_for_spawned_guest(
                &policy,
                &mut trend,
                candidates.iter(),
                1.0,
                &mut random
            ),
            None
        );
        assert_eq!(trend.spawn_count(), 39);
        assert_eq!(
            choose_favourite_animal_for_spawned_guest(
                &policy,
                &mut trend,
                candidates.iter(),
                2.0,
                &mut random
            ),
            Some(next)
        );
        assert_eq!(trend.species(), [Some(first), Some(next)]);
        assert_eq!(trend.spawn_count(), 0);
        assert_eq!(trend.spawn_threshold_index(), 1);
        for _ in 0..10 {
            choose_favourite_animal_for_spawned_guest(
                &policy,
                &mut trend,
                candidates.iter(),
                2.0,
                &mut random,
            );
        }
        assert_eq!(trend.species(), [Some(next), None]);
        assert_eq!(trend.spawn_threshold_index(), 0);
    }
}
