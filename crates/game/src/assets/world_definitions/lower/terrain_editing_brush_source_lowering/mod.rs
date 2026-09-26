use super::terrain_editing_brush_source_vocabulary::{brush_falloff, brush_operation};
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    array, id, required, required_number,
};
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use openzt2_game_data::world_definitions::terrain_editing_brushes::TerrainEditingBrushDefinition;

pub(super) fn bind_brush(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    output.document.brushes.push(TerrainEditingBrushDefinition {
        id: id(record.key),
        radius_cm: array(record, &["minimumRadiusCm", "maximumRadiusCm"], [0_u16; 2])?,
        strength_permille: required_number(record, &["strengthPermille", "strength"])?,
        falloff: brush_falloff(record.value(&["falloff"]).unwrap_or("linear"))?,
        operation: brush_operation(required(record, &["operation", "brushType"])?)?,
    });
    Ok(())
}
