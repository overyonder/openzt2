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
pub(crate) struct IncrementalZooCleanlinessTotals {
    pub maintainable_condition_permille_sum: u64,
    pub maintainable_object_count: u32,
    pub contained_waste_units: u64,
    pub contained_waste_capacity_units: u64,
    pub uncontained_waste_units: u64,
    pub requires_recalculation: bool,
}

#[derive(Component)]
pub(crate) struct MaintenanceRepairStaffJobKindMarker;

#[derive(Component)]
pub(crate) struct WasteContainerEmptyingStaffJobKindMarker;

#[derive(Component)]
pub(crate) struct LooseLitterSweepingStaffJobKindMarker;

#[derive(Component)]
pub(crate) struct AnimalHabitatWasteCleaningStaffJobKindMarker;

#[derive(Component)]
pub(crate) struct AuthoredMaintenanceFactsResolutionMarker;

#[derive(Component)]
pub(crate) struct AnimalWasteProductionScheduleResolutionMarker;
