use super::money_types::Money;
use openzt2_game_data::world_definitions::economy_transactions::{
    EconomyTransactionCostBasis, EconomyTransactionDefinition,
};

pub(super) fn evaluate_authored_transaction_amount(
    transaction: &EconomyTransactionDefinition,
    price_index: Option<usize>,
    parent_cents: i64,
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
        #[allow(
            clippy::cast_precision_loss,
            reason = "authored source costs are f32; convert the canonical cents for its parent-cost formula"
        )]
        EconomyTransactionCostBasis::Parent => parent_cents as f64 / 100.0,
        #[allow(
            clippy::cast_precision_loss,
            reason = "authored source costs are f32; convert the canonical cents for its percentage formula"
        )]
        EconomyTransactionCostBasis::PercentParent => {
            parent_cents as f64 * f64::from(transaction.cost) / 10_000.0
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

pub(super) fn authored_currency_to_money(amount: f64) -> Option<Money> {
    let cents = (amount * 100.0).round();
    if !cents.is_finite() || cents < 0.0 || cents >= 9_223_372_036_854_775_808.0 {
        return None;
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "rounded finite nonnegative cents checked against the i64 upper boundary"
    )]
    Some(Money(cents as i64))
}
