use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::AssetId;

use crate::economy::{
    authored_transaction_amount::{
        authored_currency_to_money, evaluate_authored_transaction_amount,
    },
    money_types::Money,
};

pub fn station_fare(
    definitions: WorldDefinitionsView<'_>,
    station: AssetId,
    price_index: Option<u8>,
) -> Option<Money> {
    let object = definitions.find_object(definitions.find_station(station)?.object)?;
    let name = AssetId::from_key("fare");
    let transaction = object
        .transactions
        .iter()
        .find(|entry| entry.name == name)?;
    if price_index.is_some() && transaction.cost_choices.is_empty() {
        return None;
    }
    evaluate_authored_transaction_amount(
        transaction,
        price_index.map(usize::from),
        Money::from_cents(object.price_cents),
        None,
        || None,
    )
    .and_then(authored_currency_to_money)
}
