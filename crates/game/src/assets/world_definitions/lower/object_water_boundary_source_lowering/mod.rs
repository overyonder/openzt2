use openzt2_game_data::world_definitions::world_objects::{
    WorldObjectDefinition, WorldObjectPropertyFlags,
};

use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};

use super::source_element_tree_search::authored_type_family_component_attribute;

pub(super) fn apply_authored_water_boundary_property(
    record: &RecordView<'_, '_>,
    object: &mut WorldObjectDefinition,
) -> Result<(), BindError> {
    let Some(value) =
        authored_type_family_component_attribute(record, "BFGCollisionData", "isWaterBoundary")
    else {
        return Ok(());
    };
    let enabled = match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" => true,
        "false" | "no" | "0" => false,
        _ => return Err(BindError::record(record, "invalid isWaterBoundary value")),
    };
    let bit = WorldObjectPropertyFlags::WATER_BOUNDARY.raw_flag_bits();
    let bits = object.properties.raw_flag_bits() & !bit;
    object.properties =
        WorldObjectPropertyFlags::from_raw_flag_bits(bits | if enabled { bit } else { 0 })
            .expect("water boundary is a declared object property");
    Ok(())
}
