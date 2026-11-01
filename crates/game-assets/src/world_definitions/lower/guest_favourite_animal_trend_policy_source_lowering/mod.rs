use crate::source_document::resolved_source_record_index::{BindError, RecordView};
use openzt2_game_data::world_definitions::guest_favourite_animal_trend_policy::GuestFavouriteAnimalTrendPolicy;

use super::world_definition_source_value_reading_and_conversion::required_number;

pub(super) fn lower_guest_favourite_animal_trend_policy(
    record: &RecordView<'_, '_>,
) -> Result<GuestFavouriteAnimalTrendPolicy, BindError> {
    Ok(GuestFavouriteAnimalTrendPolicy {
        trend_group_percent: required_number(record, &["trendGroupPct"])?,
        trend_setter_percent: required_number(record, &["trendSetterPct"])?,
        spawn_thresholds: [
            required_number(record, &["spawnThreshold1"])?,
            required_number(record, &["spawnThreshold2"])?,
        ],
    })
}
