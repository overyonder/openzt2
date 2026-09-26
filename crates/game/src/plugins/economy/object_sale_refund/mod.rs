use super::{
    authored_transaction_amount::{
        authored_currency_to_money, evaluate_authored_transaction_amount,
    },
    money_types::Money,
};
use openzt2_game_data::{
    world_definitions::{
        economy_transactions::EconomyTransactionKind, world_objects::WorldObjectDefinition,
    },
    AssetId,
};

pub(crate) fn calculate_authored_object_sale_refund(
    object: &WorldObjectDefinition,
) -> Option<Money> {
    let Some(transaction) = object
        .transactions
        .iter()
        .find(|transaction| transaction.name == AssetId::from_key("destroy"))
    else {
        return Some(Money::ZERO);
    };
    if transaction.kind != EconomyTransactionKind::Credit {
        return None;
    }
    let amount =
        evaluate_authored_transaction_amount(transaction, None, object.price_cents, None, || None)?;
    authored_currency_to_money(amount)
}
