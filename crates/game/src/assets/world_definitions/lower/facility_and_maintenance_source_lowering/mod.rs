use super::object_facility_staff_and_guest_source_vocabulary::{service_kind, staff_role_kind};
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    array, asset, element_asset, element_number, id, money_cents_i32_or, number_or, optional_asset,
    required, required_number,
};
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use openzt2_game_data::world_definitions::facilities_and_maintenance::{
    CleanlinessPolicy, FacilityDefinition, FacilityPaymentTrigger,
    FacilityServiceMaintenanceEffect, MaintenanceDefinition,
};

pub(super) fn bind_facility(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    output.document.facilities.push(FacilityDefinition {
        id: id(record.key),
        object: asset(record, &["object", "entity", "definition"]),
        service: service_kind(required(
            record,
            &["service", "serviceType", "facilityType"],
        )?)?,
        capacity: number_or(record, &["capacity", "maxGuests"], 0)?,
        service_ticks: number_or(record, &["serviceTicks", "useDurationTicks"], 0)?,
        payment_trigger: FacilityPaymentTrigger::TimedTicks,
        price_cents: money_cents_i32_or(record, &["priceCents"], &["fee", "servicePrice"])?,
        staffing: staff_role_kind(
            record
                .value(&["staffing", "requiredStaff"])
                .unwrap_or("none"),
        )?,
        inventory_capacity: number_or(record, &["inventoryCapacity"], 0)?,
        inventory_units_per_service: number_or(record, &["inventoryUnitsPerService"], 0)?,
        inventory_restock_per_zoo_day: number_or(record, &["inventoryRestockPerZooDay"], 0)?,
    });
    Ok(())
}

pub(super) fn bind_maintenance(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let service_effects = record
        .children_named(&["serviceEffect", "maintenanceEffect"])
        .into_iter()
        .map(|effect| {
            Ok(FacilityServiceMaintenanceEffect {
                service: element_asset(&effect, &["service"]),
                condition_loss_permille: element_number(&effect, &["conditionLossPermille"], 0)?,
                contained_waste_units: element_number(&effect, &["containedWasteUnits"], 0)?,
                loose_litter_units: element_number(&effect, &["looseLitterUnits"], 0)?,
            })
        })
        .collect::<Result<Vec<_>, BindError>>()?;
    output
        .document
        .maintenance_definitions
        .push(MaintenanceDefinition {
            id: id(record.key),
            object: asset(record, &["object", "entity"]),
            initial_condition_permille: number_or(
                record,
                &["initialConditionPermille", "initialCondition"],
                1000,
            )?,
            deterioration_per_zoo_day_permille: number_or(
                record,
                &["deteriorationPerZooDayPermille", "dailyDeterioration"],
                0,
            )?,
            repair_below_permille: number_or(
                record,
                &["repairBelowPermille", "repairThreshold"],
                0,
            )?,
            waste_capacity_units: number_or(record, &["wasteCapacityUnits", "wasteCapacity"], 0)?,
            empty_at_units: number_or(record, &["emptyAtUnits", "emptyThreshold"], 0)?,
            litter_definition: optional_asset(record, &["litterDefinition", "litter"]),
            litter_local_offset_cm: array(
                record,
                &["litterOffsetX", "litterOffsetY", "litterOffsetZ"],
                [0_i16; 3],
            )?,
            service_effects,
        });
    Ok(())
}

pub(super) fn bind_cleanliness(
    record: &RecordView<'_, '_>,
) -> Result<CleanlinessPolicy, BindError> {
    Ok(CleanlinessPolicy {
        condition_weight: required_number(record, &["conditionWeight"])?,
        waste_weight: required_number(record, &["wasteWeight"])?,
        litter_weight: required_number(record, &["litterWeight"])?,
        litter_reference_units: required_number(record, &["litterReferenceUnits"])?,
    })
}
