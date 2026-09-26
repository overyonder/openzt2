use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

pub(crate) fn find_catalogue_definition_index(
    definitions: WorldDefinitionsView<'_>,
    definition: AssetId,
) -> Option<u32> {
    definitions
        .catalogue()
        .position(|row| row.definition.0 == definition.0)
        .and_then(|index| u32::try_from(index).ok())
}
