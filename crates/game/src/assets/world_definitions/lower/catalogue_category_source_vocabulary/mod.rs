use super::world_definition_source_value_reading_and_conversion::enum_value;
use crate::assets::source_document::resolved_source_record_index::BindError;
use openzt2_game_data::world_definitions::catalogue_and_progression::catalogue_definition_types::CatalogueCategory;

pub(super) fn catalogue_category(v: &str) -> Result<CatalogueCategory, BindError> {
    enum_value(
        v,
        &[
            ("animals", CatalogueCategory::Animals),
            ("biomes", CatalogueCategory::Biomes),
            ("scenery", CatalogueCategory::Scenery),
            ("facilities", CatalogueCategory::Facilities),
            ("fences", CatalogueCategory::Fences),
            ("paths", CatalogueCategory::Paths),
            ("staff", CatalogueCategory::Staff),
            ("tanks", CatalogueCategory::Tanks),
            ("shows", CatalogueCategory::Shows),
            ("transport", CatalogueCategory::Transport),
        ],
        "catalogue category",
    )
}
