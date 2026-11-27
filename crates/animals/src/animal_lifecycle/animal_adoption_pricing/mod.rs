use openzt2_game_data::species::Species;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::economy::money_types::Money;

pub(super) fn resolve_nonnegative_animal_adoption_cost(
    world_definitions: WorldDefinitionsView<'_>,
    species: &Species,
    multiplier_permille: u16,
) -> Option<Money> {
    world_definitions
        .find_object(species.world_definition)
        .map(|object_definition| Money::from_cents(object_definition.price_cents))
        .filter(|cost| *cost >= Money::ZERO)?
        .checked_scale(f32::from(multiplier_permille) / 1000.0)
}
