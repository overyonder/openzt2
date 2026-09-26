use super::source_element_tree_search::authored_type_family_component_attribute;
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::world_definitions::world_objects::{
    WorldObjectContainerContent, WorldObjectContainerQuantityDefinition,
};

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
    Ok(Some(WorldObjectContainerQuantityDefinition {
        initial_q16: (level * 65_536.0).round() as i32,
        content,
    }))
}
