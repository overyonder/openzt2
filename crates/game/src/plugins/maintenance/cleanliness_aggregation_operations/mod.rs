use bevy::ecs::{lifecycle::HookContext, world::DeferredWorld};
use openzt2_game_data::world_definitions::facilities_and_maintenance::CleanlinessPolicy;

use super::maintenance_types::{
    AnimalHabitatWaste, FacilityContainedWaste, IncrementalZooCleanlinessTotals, LooseLitterWaste,
    MaintainableObjectConditionPermille,
};

impl IncrementalZooCleanlinessTotals {
    pub(crate) fn add_maintainable_object_condition(
        &mut self,
        maintainable_object_condition: MaintainableObjectConditionPermille,
    ) {
        self.maintainable_condition_permille_sum = self
            .maintainable_condition_permille_sum
            .saturating_add(u64::from(maintainable_object_condition.0));
        self.maintainable_object_count = self.maintainable_object_count.saturating_add(1);
        self.requires_recalculation = true;
    }

    pub(crate) fn remove_maintainable_object_condition(
        &mut self,
        maintainable_object_condition: MaintainableObjectConditionPermille,
    ) {
        self.maintainable_condition_permille_sum = self
            .maintainable_condition_permille_sum
            .saturating_sub(u64::from(maintainable_object_condition.0));
        self.maintainable_object_count = self.maintainable_object_count.saturating_sub(1);
        self.requires_recalculation = true;
    }

    pub(crate) fn replace_maintainable_object_condition(
        &mut self,
        previous_condition: MaintainableObjectConditionPermille,
        current_condition: MaintainableObjectConditionPermille,
    ) {
        self.maintainable_condition_permille_sum = self
            .maintainable_condition_permille_sum
            .saturating_sub(u64::from(previous_condition.0))
            .saturating_add(u64::from(current_condition.0));
        self.requires_recalculation |= previous_condition != current_condition;
    }

    pub(crate) fn add_facility_contained_waste(&mut self, contained_waste: FacilityContainedWaste) {
        self.contained_waste_units = self
            .contained_waste_units
            .saturating_add(u64::from(contained_waste.contained_waste_units));
        self.contained_waste_capacity_units = self
            .contained_waste_capacity_units
            .saturating_add(u64::from(contained_waste.contained_waste_capacity_units));
        self.requires_recalculation = true;
    }

    pub(crate) fn remove_facility_contained_waste(
        &mut self,
        contained_waste: FacilityContainedWaste,
    ) {
        self.contained_waste_units = self
            .contained_waste_units
            .saturating_sub(u64::from(contained_waste.contained_waste_units));
        self.contained_waste_capacity_units = self
            .contained_waste_capacity_units
            .saturating_sub(u64::from(contained_waste.contained_waste_capacity_units));
        self.requires_recalculation = true;
    }

    pub(crate) fn replace_facility_contained_waste(
        &mut self,
        previous_contained_waste: FacilityContainedWaste,
        current_contained_waste: FacilityContainedWaste,
    ) {
        self.contained_waste_units = self
            .contained_waste_units
            .saturating_sub(u64::from(previous_contained_waste.contained_waste_units))
            .saturating_add(u64::from(current_contained_waste.contained_waste_units));
        self.contained_waste_capacity_units = self
            .contained_waste_capacity_units
            .saturating_sub(u64::from(
                previous_contained_waste.contained_waste_capacity_units,
            ))
            .saturating_add(u64::from(
                current_contained_waste.contained_waste_capacity_units,
            ));
        self.requires_recalculation |= previous_contained_waste != current_contained_waste;
    }

    pub(crate) fn add_uncontained_waste_units(&mut self, added_waste_units: u16) {
        self.uncontained_waste_units = self
            .uncontained_waste_units
            .saturating_add(u64::from(added_waste_units));
        self.requires_recalculation = true;
    }

    pub(crate) fn remove_uncontained_waste_units(&mut self, removed_waste_units: u16) {
        self.uncontained_waste_units = self
            .uncontained_waste_units
            .saturating_sub(u64::from(removed_waste_units));
        self.requires_recalculation = true;
    }

    pub(crate) fn replace_uncontained_waste_units(
        &mut self,
        previous_waste_units: u16,
        current_waste_units: u16,
    ) {
        self.uncontained_waste_units = self
            .uncontained_waste_units
            .saturating_sub(u64::from(previous_waste_units))
            .saturating_add(u64::from(current_waste_units));
        self.requires_recalculation |= previous_waste_units != current_waste_units;
    }
}

macro_rules! define_cleanliness_total_component_lifecycle_hooks {
    (
        $add_hook:ident,
        $remove_hook:ident,
        $component_type:ty,
        $add_total_method:ident,
        $remove_total_method:ident,
        $component_value:expr
    ) => {
        pub(crate) fn $add_hook(mut deferred_world: DeferredWorld, hook_context: HookContext) {
            let Some(component) = deferred_world
                .get::<$component_type>(hook_context.entity)
                .copied()
            else {
                return;
            };
            if let Some(mut cleanliness_totals) =
                deferred_world.get_resource_mut::<IncrementalZooCleanlinessTotals>()
            {
                cleanliness_totals.$add_total_method(($component_value)(component));
            }
        }

        pub(crate) fn $remove_hook(mut deferred_world: DeferredWorld, hook_context: HookContext) {
            let Some(component) = deferred_world
                .get::<$component_type>(hook_context.entity)
                .copied()
            else {
                return;
            };
            if let Some(mut cleanliness_totals) =
                deferred_world.get_resource_mut::<IncrementalZooCleanlinessTotals>()
            {
                cleanliness_totals.$remove_total_method(($component_value)(component));
            }
        }
    };
}

define_cleanliness_total_component_lifecycle_hooks!(
    add_maintainable_object_condition_to_cleanliness_totals,
    remove_maintainable_object_condition_from_cleanliness_totals,
    MaintainableObjectConditionPermille,
    add_maintainable_object_condition,
    remove_maintainable_object_condition,
    |maintainable_object_condition: MaintainableObjectConditionPermille| {
        maintainable_object_condition
    }
);
define_cleanliness_total_component_lifecycle_hooks!(
    add_facility_waste_container_to_cleanliness_totals,
    remove_facility_waste_container_from_cleanliness_totals,
    FacilityContainedWaste,
    add_facility_contained_waste,
    remove_facility_contained_waste,
    |facility_contained_waste: FacilityContainedWaste| facility_contained_waste
);
define_cleanliness_total_component_lifecycle_hooks!(
    add_loose_litter_to_cleanliness_totals,
    remove_loose_litter_from_cleanliness_totals,
    LooseLitterWaste,
    add_uncontained_waste_units,
    remove_uncontained_waste_units,
    |loose_litter_waste: LooseLitterWaste| loose_litter_waste.uncontained_waste_units
);
define_cleanliness_total_component_lifecycle_hooks!(
    add_animal_habitat_waste_to_cleanliness_totals,
    remove_animal_habitat_waste_from_cleanliness_totals,
    AnimalHabitatWaste,
    add_uncontained_waste_units,
    remove_uncontained_waste_units,
    |animal_habitat_waste: AnimalHabitatWaste| animal_habitat_waste.uncontained_waste_units
);

#[inline]
pub(crate) const fn divide_unsigned_ratio_with_half_up_rounding(
    numerator: u64,
    denominator: u64,
) -> u64 {
    if denominator == 0 {
        return 0;
    }
    numerator.saturating_add(denominator / 2) / denominator
}

pub(crate) fn calculate_weighted_zoo_cleanliness_permille(
    cleanliness_totals: IncrementalZooCleanlinessTotals,
    cleanliness_policy: &CleanlinessPolicy,
) -> u16 {
    calculate_weighted_zoo_cleanliness_permille_from_explicit_weights(
        cleanliness_totals,
        cleanliness_policy.condition_weight,
        cleanliness_policy.waste_weight,
        cleanliness_policy.litter_weight,
        cleanliness_policy.litter_reference_units,
    )
}

pub(crate) fn calculate_weighted_zoo_cleanliness_permille_from_explicit_weights(
    cleanliness_totals: IncrementalZooCleanlinessTotals,
    maintainable_condition_weight: u16,
    contained_waste_weight: u16,
    uncontained_waste_weight: u16,
    uncontained_waste_reference_units: u32,
) -> u16 {
    let maintainable_condition_cleanliness_permille =
        if cleanliness_totals.maintainable_object_count == 0 {
            1_000
        } else {
            divide_unsigned_ratio_with_half_up_rounding(
                cleanliness_totals.maintainable_condition_permille_sum,
                u64::from(cleanliness_totals.maintainable_object_count),
            )
            .min(1_000)
        };
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
    let maintainable_condition_weight = u64::from(maintainable_condition_weight);
    let contained_waste_weight = u64::from(contained_waste_weight);
    let uncontained_waste_weight = u64::from(uncontained_waste_weight);
    let total_cleanliness_weight = maintainable_condition_weight
        .saturating_add(contained_waste_weight)
        .saturating_add(uncontained_waste_weight);
    divide_unsigned_ratio_with_half_up_rounding(
        maintainable_condition_cleanliness_permille
            .saturating_mul(maintainable_condition_weight)
            .saturating_add(
                contained_waste_cleanliness_permille.saturating_mul(contained_waste_weight),
            )
            .saturating_add(
                uncontained_waste_cleanliness_permille.saturating_mul(uncontained_waste_weight),
            ),
        total_cleanliness_weight,
    )
    .min(1_000) as u16
}
