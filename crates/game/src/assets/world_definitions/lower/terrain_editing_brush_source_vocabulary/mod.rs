use super::world_definition_source_value_reading_and_conversion::enum_value;
use crate::assets::source_document::resolved_source_record_index::BindError;
use openzt2_game_data::world_definitions::terrain_editing_brushes::{
    TerrainEditingBrushFalloff, TerrainEditingBrushOperation,
};

pub(super) fn brush_falloff(v: &str) -> Result<TerrainEditingBrushFalloff, BindError> {
    enum_value(
        v,
        &[
            ("constant", TerrainEditingBrushFalloff::Constant),
            ("linear", TerrainEditingBrushFalloff::Linear),
            ("smooth", TerrainEditingBrushFalloff::Smooth),
        ],
        "brush falloff",
    )
}
pub(super) fn brush_operation(v: &str) -> Result<TerrainEditingBrushOperation, BindError> {
    enum_value(
        v,
        &[
            ("raise", TerrainEditingBrushOperation::Raise),
            ("lower", TerrainEditingBrushOperation::Lower),
            ("flatten", TerrainEditingBrushOperation::Flatten),
            ("smooth", TerrainEditingBrushOperation::Smooth),
            ("paintbiome", TerrainEditingBrushOperation::PaintBiome),
            ("addwater", TerrainEditingBrushOperation::AddWater),
            ("removewater", TerrainEditingBrushOperation::RemoveWater),
        ],
        "brush operation",
    )
}
