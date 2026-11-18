use openzt2_game_data::AssetId;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

pub fn find_catalogue_definition_index(
    definitions: WorldDefinitionsView<'_>,
    definition: AssetId,
) -> Option<u32> {
    definitions
        .catalogue_entry_for_definition(definition)
        .and_then(|(index, _)| u32::try_from(index).ok())
}
