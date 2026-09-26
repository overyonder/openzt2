use openzt2_game_data::world_definitions::catalogue_and_progression::zoo_rating_fame_and_award_definition_types::{Comparison,RatingInputKind};
use crate::assets::source_document::resolved_source_record_index::BindError;
use super::world_definition_source_value_reading_and_conversion::enum_value;

pub(super) fn rating_input(v: &str) -> Result<RatingInputKind, BindError> {
    enum_value(
        v,
        &[
            ("animalwelfare", RatingInputKind::AnimalWelfare),
            ("guestsatisfaction", RatingInputKind::GuestSatisfaction),
            ("education", RatingInputKind::Education),
            ("variety", RatingInputKind::Variety),
            ("scenery", RatingInputKind::Scenery),
            ("finance", RatingInputKind::Finance),
            ("cleanliness", RatingInputKind::Cleanliness),
        ],
        "rating input",
    )
}
pub(super) fn comparison(v: &str) -> Result<Comparison, BindError> {
    enum_value(
        v,
        &[
            ("atleast", Comparison::AtLeast),
            ("greaterthanorequal", Comparison::AtLeast),
            ("atmost", Comparison::AtMost),
            ("lessthanorequal", Comparison::AtMost),
            ("equal", Comparison::Equal),
        ],
        "comparison",
    )
}
