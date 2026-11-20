use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::world_definitions::facilities_and_maintenance::CleanlinessPolicy;

use super::maintenance_types::{
    AnimalHabitatWaste, FacilityContainedWaste, LooseLitterWaste, ZooCleanlinessPermille,
};

#[derive(Resource, Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) struct ZooCleanlinessTotals {
    pub(super) contained_waste_units: u64,
    pub(super) contained_waste_capacity_units: u64,
    pub(super) uncontained_waste_units: u64,
}

pub(super) fn aggregate_changed_waste_into_zoo_cleanliness_totals(
    containers: Query<&FacilityContainedWaste>,
    litter: Query<&LooseLitterWaste>,
    animal_waste: Query<&AnimalHabitatWaste>,
    changed_containers: Query<(), Changed<FacilityContainedWaste>>,
    changed_litter: Query<(), Changed<LooseLitterWaste>>,
    changed_animal_waste: Query<(), Changed<AnimalHabitatWaste>>,
    mut removed_containers: RemovedComponents<FacilityContainedWaste>,
    mut removed_litter: RemovedComponents<LooseLitterWaste>,
    mut removed_animal_waste: RemovedComponents<AnimalHabitatWaste>,
    mut totals: ResMut<ZooCleanlinessTotals>,
) {
    let mut current = *totals;
    let containers_removed = removed_containers.read().count() != 0;
    let litter_removed = removed_litter.read().count() != 0;
    let animal_waste_removed = removed_animal_waste.read().count() != 0;
    if containers_removed || !changed_containers.is_empty() {
        (
            current.contained_waste_units,
            current.contained_waste_capacity_units,
        ) = containers
            .iter()
            .fold((0_u64, 0_u64), |(units, capacity), container| {
                (
                    units.saturating_add(u64::from(container.contained_waste_units)),
                    capacity.saturating_add(u64::from(container.contained_waste_capacity_units)),
                )
            });
    }
    if litter_removed
        || animal_waste_removed
        || !changed_litter.is_empty()
        || !changed_animal_waste.is_empty()
    {
        current.uncontained_waste_units = litter
            .iter()
            .map(|waste| u64::from(waste.uncontained_waste_units))
            .chain(
                animal_waste
                    .iter()
                    .map(|waste| u64::from(waste.uncontained_waste_units)),
            )
            .fold(0, u64::saturating_add);
    }
    totals.set_if_neq(current);
}

pub(super) fn project_zoo_cleanliness_from_totals_and_policy(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    cleanliness_totals: Res<ZooCleanlinessTotals>,
    mut zoo_cleanliness: ResMut<ZooCleanlinessPermille>,
) {
    if !cleanliness_totals.is_changed()
        && !active_world_definitions.is_changed()
        && !world_definition_assets.is_changed()
    {
        return;
    }
    let projected = active_world_definitions
        .get(&world_definition_assets)
        .and_then(|world_definitions| world_definitions.cleanliness_policy())
        .map(|policy| calculate_weighted_zoo_cleanliness_permille(*cleanliness_totals, policy));
    zoo_cleanliness.set_if_neq(ZooCleanlinessPermille(projected));
}

#[inline]

pub(super) const fn divide_unsigned_ratio_with_half_up_rounding(
    numerator: u64,
    denominator: u64,
) -> u64 {
    if denominator == 0 {
        return 0;
    }
    numerator.saturating_add(denominator / 2) / denominator
}

pub(super) fn calculate_weighted_zoo_cleanliness_permille(
    cleanliness_totals: ZooCleanlinessTotals,
    cleanliness_policy: &CleanlinessPolicy,
) -> u16 {
    calculate_weighted_zoo_cleanliness_permille_from_explicit_weights(
        cleanliness_totals,
        cleanliness_policy.waste_weight,
        cleanliness_policy.litter_weight,
        cleanliness_policy.litter_reference_units,
    )
}

pub(super) fn calculate_weighted_zoo_cleanliness_permille_from_explicit_weights(
    cleanliness_totals: ZooCleanlinessTotals,
    contained_waste_weight: u16,
    uncontained_waste_weight: u16,
    uncontained_waste_reference_units: u32,
) -> u16 {
    let contained_waste_cleanliness_permille =
        if cleanliness_totals.contained_waste_capacity_units == 0 {
            1_000
        } else {
            1_000_u64.saturating_sub(
                divide_unsigned_ratio_with_half_up_rounding(
                    cleanliness_totals
                        .contained_waste_units
                        .saturating_mul(1_000),
                    cleanliness_totals.contained_waste_capacity_units,
                )
                .min(1_000),
            )
        };
    let uncontained_waste_cleanliness_permille = 1_000_u64.saturating_sub(
        divide_unsigned_ratio_with_half_up_rounding(
            cleanliness_totals
                .uncontained_waste_units
                .saturating_mul(1_000),
            u64::from(uncontained_waste_reference_units),
        )
        .min(1_000),
    );
    let contained_waste_weight = u64::from(contained_waste_weight);
    let uncontained_waste_weight = u64::from(uncontained_waste_weight);
    let total_cleanliness_weight = contained_waste_weight.saturating_add(uncontained_waste_weight);
    divide_unsigned_ratio_with_half_up_rounding(
        contained_waste_cleanliness_permille
            .saturating_mul(contained_waste_weight)
            .saturating_add(
                uncontained_waste_cleanliness_permille.saturating_mul(uncontained_waste_weight),
            ),
        total_cleanliness_weight,
    )
    .min(1_000) as u16
}
