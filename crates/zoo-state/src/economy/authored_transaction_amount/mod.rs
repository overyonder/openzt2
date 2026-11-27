use super::money_types::Money;
use openzt2_game_data::world_definitions::economy_transactions::{
    EconomyTransactionCostBasis, EconomyTransactionDefinition,
};

pub fn evaluate_authored_transaction_amount(
    transaction: &EconomyTransactionDefinition,
    price_index: Option<usize>,
    parent: Money,
    override_amount: Option<f32>,
    mut random_draw: impl FnMut() -> Option<u32>,
) -> Option<f64> {
    if let Some(amount) = override_amount {
        return Some(f64::from(amount));
    }
    let amount = match transaction.cost_basis {
        EconomyTransactionCostBasis::Fixed => {
            if transaction.cost_choices.is_empty() {
                f64::from(transaction.cost)
            } else {
                f64::from(
                    *transaction
                        .cost_choices
                        .get(price_index.unwrap_or(transaction.initial_cost_index))?,
                )
            }
        }
        EconomyTransactionCostBasis::Parent => f64::from(parent.dollars()),
        EconomyTransactionCostBasis::PercentParent => {
            f64::from(parent.dollars()) * f64::from(transaction.cost) / 100.0
        }
        EconomyTransactionCostBasis::Random => {
            let minimum = f64::from(transaction.minimum_cost);
            let maximum = f64::from(transaction.maximum_cost);
            if maximum <= minimum {
                minimum
            } else if transaction.cost_step <= 0.0 {
                minimum + (maximum - minimum) * f64::from(random_draw()?) / f64::from(u32::MAX)
            } else {
                let step = f64::from(transaction.cost_step);
                let steps = ((maximum - minimum) / step).floor();
                if steps >= f64::from(u32::MAX) {
                    return None;
                }
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "nonnegative integral step count checked below u32::MAX"
                )]
                let choices = steps as u32 + 1;
                minimum + f64::from(random_draw()? % choices) * step
            }
        }
    };
    amount.is_finite().then_some(amount)
}

pub fn authored_currency_to_money(amount: f64) -> Option<Money> {
    Money::from_dollars(amount as f32).filter(|&amount| amount >= Money::ZERO)
}
