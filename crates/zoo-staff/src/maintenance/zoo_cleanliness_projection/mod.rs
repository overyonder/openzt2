use bevy::{platform::collections::HashMap, prelude::*};
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::world_definitions::facilities_and_maintenance::CleanlinessPolicy;

use super::maintenance_types::{
    AnimalHabitatWaste, FacilityContainedWaste, LooseLitterWaste, ZooCleanlinessPermille,
};

#[derive(Component, Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) struct ZooCleanlinessTotals {
    pub(super) contained_waste_units: u64,
    pub(super) contained_waste_capacity_units: u64,
    pub(super) uncontained_waste_units: u64,
}

pub(super) fn aggregate_changed_waste_into_zoo_cleanliness_totals(
    waste: Query<
        (
            &WorldMember,
            Option<&FacilityContainedWaste>,
            Option<&LooseLitterWaste>,
            Option<&AnimalHabitatWaste>,
        ),
        Or<(
            With<FacilityContainedWaste>,
            With<LooseLitterWaste>,
            With<AnimalHabitatWaste>,
        )>,
    >,
    changed_waste: Query<
        (),
        (
            Or<(
                With<FacilityContainedWaste>,
                With<LooseLitterWaste>,
                With<AnimalHabitatWaste>,
            )>,
            Or<(
                Changed<FacilityContainedWaste>,
                Changed<LooseLitterWaste>,
                Changed<AnimalHabitatWaste>,
                Changed<WorldMember>,
            )>,
        ),
    >,
    waste_entities: Query<
        (),
        Or<(
            With<FacilityContainedWaste>,
            With<LooseLitterWaste>,
            With<AnimalHabitatWaste>,
        )>,
    >,
    mut removed_containers: RemovedComponents<FacilityContainedWaste>,
    mut removed_litter: RemovedComponents<LooseLitterWaste>,
    mut removed_animal_waste: RemovedComponents<AnimalHabitatWaste>,
    mut removed_members: RemovedComponents<WorldMember>,
    mut world_totals: Query<(Entity, &mut ZooCleanlinessTotals), With<WorldRoot>>,
) {
    let containers_removed = removed_containers.read().count() != 0;
    let litter_removed = removed_litter.read().count() != 0;
    let animal_waste_removed = removed_animal_waste.read().count() != 0;
    let members_removed = removed_members
        .read()
        .any(|entity| waste_entities.contains(entity));
    if !containers_removed
        && !litter_removed
        && !animal_waste_removed
        && !members_removed
        && changed_waste.is_empty()
        && !world_totals.iter_mut().any(|(_, totals)| totals.is_added())
    {
        return;
    }
    let mut totals_by_world = HashMap::<Entity, ZooCleanlinessTotals>::new();
    for (member, container, litter, animal_waste) in &waste {
        let totals = totals_by_world.entry(member.root).or_default();
        if let Some(container) = container {
            totals.contained_waste_units = totals
                .contained_waste_units
                .saturating_add(u64::from(container.contained_waste_units));
            totals.contained_waste_capacity_units = totals
                .contained_waste_capacity_units
                .saturating_add(u64::from(container.contained_waste_capacity_units));
        }
        if let Some(litter) = litter {
            totals.uncontained_waste_units = totals
                .uncontained_waste_units
                .saturating_add(u64::from(litter.uncontained_waste_units));
        }
        if let Some(animal_waste) = animal_waste {
            totals.uncontained_waste_units = totals
                .uncontained_waste_units
                .saturating_add(u64::from(animal_waste.uncontained_waste_units));
        }
    }
    for (root, mut totals) in &mut world_totals {
        totals.set_if_neq(totals_by_world.remove(&root).unwrap_or_default());
    }
}

pub(super) fn project_zoo_cleanliness_from_totals_and_policy(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut worlds: Query<(Ref<ZooCleanlinessTotals>, &mut ZooCleanlinessPermille), With<WorldRoot>>,
) {
    let policy = active_world_definitions
        .get(&world_definition_assets)
        .and_then(|definitions| definitions.cleanliness_policy());
    let policy_changed =
        active_world_definitions.is_changed() || world_definition_assets.is_changed();
    for (totals, mut cleanliness) in &mut worlds {
        if totals.is_changed() || policy_changed {
            cleanliness.set_if_neq(ZooCleanlinessPermille(
                policy.map(|policy| calculate_weighted_zoo_cleanliness_permille(*totals, policy)),
            ));
        }
    }
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
