use super::staff_request_controller_source_lowering::lower_authored_staff_request_data;
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::required_number;
use crate::source_document::resolved_source_record_index::{BindError, RecordView};
use openzt2_game_data::world_definitions::staff_management::StaffAquaticTankCleaningRequestPolicy;

/// `ZTAIStaffWaterCleaningMgr` in the selected game mode's configuration.
pub(super) fn bind_staff_tank_cleaning_request(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let id = openzt2_game_data::AssetId::from_virtual_path(&record.source_path());
    if output
        .document
        .staff_tank_cleaning_requests
        .iter()
        .any(|policy| policy.id == id)
    {
        return Err(BindError::record(
            record,
            "duplicate resolved staff tank-cleaning request policy",
        ));
    }
    let dirt_threshold_percent: f32 = required_number(record, &["DirtThreshold"])?;
    if !dirt_threshold_percent.is_finite() {
        return Err(BindError::record(
            record,
            "tank-cleaning DirtThreshold is not a finite number",
        ));
    }
    let request = lower_authored_staff_request_data(record, record.source_document_element())?;
    if request.token.is_none() {
        return Err(BindError::record(
            record,
            "tank-cleaning request has no tokenKey",
        ));
    }
    output
        .document
        .staff_tank_cleaning_requests
        .push(StaffAquaticTankCleaningRequestPolicy {
            id,
            dirt_threshold: dirt_threshold_percent / 100.0,
            request,
        });
    Ok(())
}
