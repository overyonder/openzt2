use super::source_element_tree_search::authored_type_family_component_attribute;
use crate::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::world_definitions::facilities_and_maintenance::MaintenanceDefinition;
use openzt2_game_data::world_definitions::world_objects::{
    WorldObjectContainerContent, WorldObjectContainerQuantityDefinition,
};
use openzt2_game_data::AssetId;

#[cfg(test)]
mod tests;

pub(super) fn lower_authored_container_quantity(
    record: &RecordView<'_, '_>,
) -> Result<Option<WorldObjectContainerQuantityDefinition>, BindError> {
    let Some(value) =
        authored_type_family_component_attribute(record, "BFAIEntityDataInstance", "f_FoodLevel")
    else {
        return Ok(None);
    };
    let level = parse_blue_fang_source_numeric_lexeme::<f32>(value)
        .filter(|level| level.is_finite() && (0.0..=100.0).contains(level))
        .ok_or_else(|| {
            BindError::record(
                record,
                format!("invalid authored f_FoodLevel {value:?}; expected 0..=100"),
            )
        })?;
    let content =
        match authored_type_family_component_attribute(record, "BFAIEntityDataShared", "b_Water")
            .map(canonicalize_source_document_record_key)
            .as_deref()
        {
            None | Some("false" | "no" | "0") => WorldObjectContainerContent::Food,
            Some("true" | "yes" | "1") => WorldObjectContainerContent::Drink,
            Some(value) => {
                return Err(BindError::record(
                    record,
                    format!("invalid authored b_Water {value:?}"),
                ));
            }
        };
    let held_prop =
        authored_type_family_component_attribute(record, "BFAIEntityDataShared", "s_prop")
            .map(canonicalize_source_document_record_key)
            .filter(|prop| !prop.is_empty() && prop != "none")
            .map(|prop| AssetId::from_key(&prop));
    Ok(Some(WorldObjectContainerQuantityDefinition {
        initial_q16: (level * 65_536.0).round() as i32,
        content,
        held_prop,
    }))
}

/// Authored `f_TrashLevel` makes an object a waste container. Guests fill it
/// while the level is below 100 and treat it as full from there, so the
/// level is its contained waste on a capacity of 100. Emptying requests come
/// from its own staff-request controller on that level, not from a
/// maintenance threshold, and it does not wear.
pub(super) fn lower_authored_trash_container_maintenance(
    record: &RecordView<'_, '_>,
) -> Result<Option<MaintenanceDefinition>, BindError> {
    let Some(value) =
        authored_type_family_component_attribute(record, "BFAIEntityDataInstance", "f_TrashLevel")
    else {
        return Ok(None);
    };
    let level = parse_blue_fang_source_numeric_lexeme::<f32>(value)
        .filter(|level| level.is_finite() && (0.0..=100.0).contains(level))
        .ok_or_else(|| {
            BindError::record(
                record,
                format!("invalid authored f_TrashLevel {value:?}; expected 0..=100"),
            )
        })?;
    let object = super::world_definition_source_value_reading_and_conversion::id(record.key);
    Ok(Some(MaintenanceDefinition {
        id: object,
        object,
        initial_condition_permille: 1000,
        deterioration_per_zoo_day_permille: 0,
        repair_below_permille: 0,
        waste_capacity_units: TRASH_LEVEL_CAPACITY,
        initial_waste_units: level.round() as u16,
        empty_at_units: 0,
        litter_definition: None,
        litter_local_offset_cm: [0; 3],
        service_effects: Vec::new(),
    }))
}

/// The level at which authored guest disposal stops and treats a container as full.
const TRASH_LEVEL_CAPACITY: u16 = 100;
