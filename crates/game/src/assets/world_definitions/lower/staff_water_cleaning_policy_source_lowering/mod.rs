use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{id, required_element_number};
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use openzt2_game_data::world_definitions::staff_management::StaffAquaticTankWaterCleaningPolicy;

pub(super) fn bind_staff_water_cleaning(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if output.document.staff_water_cleaning.is_some() {
        return Err(BindError::record(
            record,
            "duplicate resolved staff water-cleaning policy",
        ));
    }
    let clean = record
        .children_named(&["CleanTank"])
        .first()
        .copied()
        .ok_or_else(|| BindError::record(record, "water-cleaning policy is missing CleanTank"))?;
    output.document.staff_water_cleaning = Some(StaffAquaticTankWaterCleaningPolicy {
        token: id(clean.name.as_str()),
        value: required_element_number(&clean, &["value"])?,
        localization_key: clean
            .attribute_named_any(&["locid"])
            .map(id)
            .unwrap_or_default(),
    });
    Ok(())
}
