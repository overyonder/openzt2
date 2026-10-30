use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(transparent)]
/// Remaining maintainable-object condition, where 1,000 is pristine.
pub struct MaintainableObjectConditionPermille(pub u16);

impl MaintainableObjectConditionPermille {
    pub const fn is_valid(self) -> bool {
        self.0 <= 1_000
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
/// Waste retained by a facility and the maximum amount it can retain.
pub struct FacilityContainedWaste {
    /// Current retained waste quantity.
    pub contained_waste_units: u16,
    /// Maximum retained waste quantity.
    pub contained_waste_capacity_units: u16,
}

impl FacilityContainedWaste {
    pub const fn is_valid(self) -> bool {
        self.contained_waste_capacity_units != 0
            && self.contained_waste_units <= self.contained_waste_capacity_units
    }
}

/// An authored `f_TrashLevel` change on a waste container, such as a guest
/// throwing away trash. The level is the container's contained waste.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdjustFacilityContainedWaste {
    pub facility: Entity,
    pub operation:
        openzt2_game_data::behavior::action::modification::BehaviorFactModificationOperation,
    pub delta_q16: i32,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FacilityWasteBehaviorModification;

/// Four presentation bands used by the authored trash-quantity display.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FacilityContainedWastePresentationLevel {
    #[default]
    Empty,
    OneThird,
    TwoThirds,
    Full,
}

impl FacilityContainedWastePresentationLevel {
    /// Returns the empty/thirds display level for the current quantity.
    pub const fn calculate_from_facility_contained_waste(
        facility_contained_waste: FacilityContainedWaste,
    ) -> Self {
        if facility_contained_waste.contained_waste_units == 0 {
            Self::Empty
        } else {
            let contained_waste_units_scaled_by_three =
                facility_contained_waste.contained_waste_units as u32 * 3;
            let contained_waste_capacity_units =
                facility_contained_waste.contained_waste_capacity_units as u32;
            if contained_waste_units_scaled_by_three <= contained_waste_capacity_units {
                Self::OneThird
            } else if contained_waste_units_scaled_by_three <= contained_waste_capacity_units * 2 {
                Self::TwoThirds
            } else {
                Self::Full
            }
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
/// Loose facility or guest litter which can be swept by maintenance staff.
pub struct LooseLitterWaste {
    /// Current loose litter quantity.
    pub uncontained_waste_units: u16,
}

impl LooseLitterWaste {
    pub const fn is_valid(self) -> bool {
        self.uncontained_waste_units != 0
    }
}

/// A compost bin's authored `f_PooLevel`, raised as keepers rake waste into
/// it. Absent until a script first writes it; the authored start is zero.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct CompostPooLevel(pub f32);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
/// Animal waste deposited in a habitat and awaiting maintenance staff.
pub struct AnimalHabitatWaste {
    /// Current animal-waste quantity.
    pub uncontained_waste_units: u16,
}

impl AnimalHabitatWaste {
    pub const fn is_valid(self) -> bool {
        self.uncontained_waste_units != 0
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
/// Next simulation tick at which an animal produces habitat waste.
pub struct AnimalWasteProductionSchedule {
    /// Zoo clock tick at which production is due.
    pub next_production_tick: u64,
}

/// Whole-zoo cleanliness, or `None` while its aggregation policy is unavailable.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct ZooCleanlinessPermille(pub Option<u16>);

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
/// Incremental source totals used to project whole-zoo cleanliness.
pub struct IncrementalZooCleanlinessTotals {
    pub maintainable_condition_permille_sum: u64,
    pub maintainable_object_count: u32,
    pub contained_waste_units: u64,
    pub contained_waste_capacity_units: u64,
    pub uncontained_waste_units: u64,
    pub requires_recalculation: bool,
}

#[derive(Component)]
pub struct MaintenanceRepairStaffJobKindMarker;

#[derive(Component)]
pub struct WasteContainerEmptyingStaffJobKindMarker;

#[derive(Component)]
pub struct LooseLitterSweepingStaffJobKindMarker;

/// The authored kill has consumed this job's target; retain the assignment
/// through the remaining behavior actions and their terminal outcome.
#[derive(Component)]
pub struct AppliedHabitatWasteCleanup {
    pub execution_id: u64,
}

#[derive(Component)]
pub struct AuthoredMaintenanceFactsResolutionMarker;

#[derive(Component)]
pub struct AnimalWasteProductionScheduleResolutionMarker;

impl IncrementalZooCleanlinessTotals {
    pub fn add_maintainable_object_condition(
        &mut self,
        maintainable_object_condition: MaintainableObjectConditionPermille,
    ) {
        self.maintainable_condition_permille_sum = self
            .maintainable_condition_permille_sum
            .saturating_add(u64::from(maintainable_object_condition.0));
        self.maintainable_object_count = self.maintainable_object_count.saturating_add(1);
        self.requires_recalculation = true;
    }

    pub fn remove_maintainable_object_condition(
        &mut self,
        maintainable_object_condition: MaintainableObjectConditionPermille,
    ) {
        self.maintainable_condition_permille_sum = self
            .maintainable_condition_permille_sum
            .saturating_sub(u64::from(maintainable_object_condition.0));
        self.maintainable_object_count = self.maintainable_object_count.saturating_sub(1);
        self.requires_recalculation = true;
    }

    pub fn replace_maintainable_object_condition(
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

    pub fn add_facility_contained_waste(&mut self, contained_waste: FacilityContainedWaste) {
        self.contained_waste_units = self
            .contained_waste_units
            .saturating_add(u64::from(contained_waste.contained_waste_units));
        self.contained_waste_capacity_units = self
            .contained_waste_capacity_units
            .saturating_add(u64::from(contained_waste.contained_waste_capacity_units));
        self.requires_recalculation = true;
    }

    pub fn remove_facility_contained_waste(&mut self, contained_waste: FacilityContainedWaste) {
        self.contained_waste_units = self
            .contained_waste_units
            .saturating_sub(u64::from(contained_waste.contained_waste_units));
        self.contained_waste_capacity_units = self
            .contained_waste_capacity_units
            .saturating_sub(u64::from(contained_waste.contained_waste_capacity_units));
        self.requires_recalculation = true;
    }

    pub fn replace_facility_contained_waste(
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

    pub fn add_uncontained_waste_units(&mut self, added_waste_units: u16) {
        self.uncontained_waste_units = self
            .uncontained_waste_units
            .saturating_add(u64::from(added_waste_units));
        self.requires_recalculation = true;
    }

    pub fn remove_uncontained_waste_units(&mut self, removed_waste_units: u16) {
        self.uncontained_waste_units = self
            .uncontained_waste_units
            .saturating_sub(u64::from(removed_waste_units));
        self.requires_recalculation = true;
    }

    pub fn replace_uncontained_waste_units(
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
