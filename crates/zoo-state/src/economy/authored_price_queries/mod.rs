use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::AssetId;
use super::money_types::Money;
pub fn find_authored_object_or_placeable_price(
    world_definitions: WorldDefinitionsView<'_>,
    definition: AssetId,
) -> Option<Money> {
    world_definitions
        .find_object(definition)
        .map(|object| Money(object.price_cents))
        .or_else(|| {
            world_definitions
                .find_placeable(definition)
                .map(|placeable| Money(placeable.price_cents))
        })
}
