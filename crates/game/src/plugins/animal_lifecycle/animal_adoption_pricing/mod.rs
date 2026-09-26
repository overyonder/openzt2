use openzt2_game_data::species::Species;

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::economy::money_types::Money;

pub(super) fn resolve_nonnegative_animal_adoption_cost(
    world_definitions: WorldDefinitionsView<'_>,
    species: &Species,
) -> Option<Money> {
    world_definitions
        .find_object(species.world_definition)
        .map(|object_definition| Money(object_definition.price_cents))
        .filter(|cost| cost.0 >= 0)
}
