//! Authored guest-service facility, inventory, deterioration, waste, and cleanliness definitions.

use crate::AssetId;

use super::staff_management::StaffRoleKind;

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum FacilityServiceKind {
    Food,
    Drink,
    Toilet,
    Gift,
    Education,
    Adoption,
    Transport,
    Maintenance,
    Show,
    Laboratory,
}
/// Determines when a guest pays for a facility service.
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum FacilityPaymentTrigger {
    /// The guest's buy behavior triggers payment.
    AuthoredBehavior,
    /// A fixed per-service duration charges when the service timer expires.
    TimedTicks,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct FacilityDefinition {
    pub id: AssetId,
    pub object: AssetId,
    pub service: FacilityServiceKind,
    pub capacity: u16,
    pub service_ticks: u32,
    pub payment_trigger: FacilityPaymentTrigger,
    pub price_cents: i32,
    pub staffing: StaffRoleKind,
    pub inventory_capacity: u16,
    pub inventory_units_per_service: u16,
    pub inventory_restock_per_zoo_day: u16,
}
#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct MaintenanceDefinition {
    pub id: AssetId,
    pub object: AssetId,
    pub initial_condition_permille: u16,
    pub deterioration_per_zoo_day_permille: u16,
    pub repair_below_permille: u16,
    pub waste_capacity_units: u16,
    pub empty_at_units: u16,
    pub litter_definition: Option<AssetId>,
    pub litter_local_offset_cm: [i16; 3],
    pub service_effects: Vec<FacilityServiceMaintenanceEffect>,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct FacilityServiceMaintenanceEffect {
    pub service: AssetId,
    pub condition_loss_permille: u16,
    pub contained_waste_units: u16,
    pub loose_litter_units: u16,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct CleanlinessPolicy {
    pub condition_weight: u16,
    pub waste_weight: u16,
    pub litter_weight: u16,
    pub litter_reference_units: u32,
}
