use super::world_definition_source_value_reading_and_conversion::enum_value;
use crate::assets::source_document::resolved_source_record_index::BindError;
use openzt2_game_data::world_definitions::environment::AmbientClass;

pub(super) fn ambient_class(v: &str) -> Result<AmbientClass, BindError> {
    enum_value(
        v,
        &[
            ("air", AmbientClass::Air),
            ("ground", AmbientClass::Ground),
            ("water", AmbientClass::Water),
        ],
        "ambient class",
    )
}
