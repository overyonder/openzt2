use bevy::{
    ecs::{entity::EntityHashSet, entity_disabling::Disabled},
    prelude::*,
};
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

#[derive(Component, Clone, Copy)]
pub(super) struct WasteContribution {
    root: Entity,
    totals: ZooCleanlinessTotals,
}

impl ZooCleanlinessTotals {
    fn replace(self, previous: Self, current: Self) -> Self {
        Self {
            contained_waste_units: self.contained_waste_units - previous.contained_waste_units
                + current.contained_waste_units,
            contained_waste_capacity_units: self.contained_waste_capacity_units
                - previous.contained_waste_capacity_units
                + current.contained_waste_capacity_units,
            uncontained_waste_units: self.uncontained_waste_units
                - previous.uncontained_waste_units
                + current.uncontained_waste_units,
        }
    }
}

pub(super) fn add_waste_contribution(
    event: On<Add<WasteContribution>>,
    contributors: Query<&WasteContribution, Allow<Disabled>>,
    mut worlds: Query<&mut ZooCleanlinessTotals, Allow<Disabled>>,
) {
    let Ok(contribution) = contributors.get(event.entity) else {
        return;
    };
    if let Ok(mut totals) = worlds.get_mut(contribution.root) {
        let next = totals.replace(ZooCleanlinessTotals::default(), contribution.totals);
        totals.set_if_neq(next);
    }
}

pub(super) fn remove_waste_contribution(
    event: On<Remove<WasteContribution>>,
    contributors: Query<&WasteContribution, Allow<Disabled>>,
    mut worlds: Query<&mut ZooCleanlinessTotals, Allow<Disabled>>,
) {
    let Ok(contribution) = contributors.get(event.entity) else {
        return;
    };
    if let Ok(mut totals) = worlds.get_mut(contribution.root) {
        let next = totals.replace(contribution.totals, ZooCleanlinessTotals::default());
        totals.set_if_neq(next);
    }
}

pub(super) fn withdraw_disabled_waste_contribution(
    event: On<Add<Disabled>>,
    contributors: Query<(), (With<WasteContribution>, Allow<Disabled>)>,
    mut commands: Commands,
) {
    if contributors.contains(event.entity) {
        commands.entity(event.entity).remove::<WasteContribution>();
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn publish_changed_waste_contributions(
    mut commands: Commands,
    changed: Query<
        Entity,
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
    (mut removed_container, mut removed_litter, mut removed_dung, mut left_world, mut reenabled): (
        RemovedComponents<FacilityContainedWaste>,
        RemovedComponents<LooseLitterWaste>,
        RemovedComponents<AnimalHabitatWaste>,
        RemovedComponents<WorldMember>,
        RemovedComponents<Disabled>,
    ),
    mut contributors: Query<
        (
            Option<&WorldMember>,
            Option<&FacilityContainedWaste>,
            Option<&LooseLitterWaste>,
            Option<&AnimalHabitatWaste>,
            Option<&mut WasteContribution>,
        ),
        Or<(
            With<FacilityContainedWaste>,
            With<LooseLitterWaste>,
            With<AnimalHabitatWaste>,
            With<WasteContribution>,
        )>,
    >,
    mut worlds: Query<&mut ZooCleanlinessTotals, Allow<Disabled>>,
) {
    let affected: EntityHashSet = changed
        .iter()
        .chain(removed_container.read())
        .chain(removed_litter.read())
        .chain(removed_dung.read())
        .chain(left_world.read())
        .chain(reenabled.read())
        .collect();
    for entity in affected {
        let Ok((member, container, litter, dung, contribution)) = contributors.get_mut(entity)
        else {
            continue;
        };
        let Some(member) =
            member.filter(|_| container.is_some() || litter.is_some() || dung.is_some())
        else {
            if contribution.is_some() {
                commands.entity(entity).remove::<WasteContribution>();
            }
            continue;
        };
        let next = ZooCleanlinessTotals {
            contained_waste_units: container
                .map_or(0, |waste| u64::from(waste.contained_waste_units)),
            contained_waste_capacity_units: container
                .map_or(0, |waste| u64::from(waste.contained_waste_capacity_units)),
            uncontained_waste_units: litter
                .map_or(0, |waste| u64::from(waste.uncontained_waste_units))
                + dung.map_or(0, |waste| u64::from(waste.uncontained_waste_units)),
        };
        if !worlds.contains(member.root) {
            if contribution.is_some() {
                commands.entity(entity).remove::<WasteContribution>();
            }
            continue;
        }
        if let Some(mut contribution) = contribution {
            if contribution.root == member.root {
                if let Ok(mut totals) = worlds.get_mut(member.root) {
                    let total = totals.replace(contribution.totals, next);
                    totals.set_if_neq(total);
                }
            } else {
                if let Ok(mut totals) = worlds.get_mut(contribution.root) {
                    let total =
                        totals.replace(contribution.totals, ZooCleanlinessTotals::default());
                    totals.set_if_neq(total);
                }
                if let Ok(mut totals) = worlds.get_mut(member.root) {
                    let total = totals.replace(ZooCleanlinessTotals::default(), next);
                    totals.set_if_neq(total);
                }
                contribution.root = member.root;
            }
            contribution
                .map_unchanged(|applied| &mut applied.totals)
                .set_if_neq(next);
        } else {
            commands.entity(entity).insert(WasteContribution {
                root: member.root,
                totals: next,
            });
        }
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
