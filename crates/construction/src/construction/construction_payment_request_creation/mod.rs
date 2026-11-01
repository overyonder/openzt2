use bevy::prelude::*;

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::economy::account_transaction_types::Account;
use openzt2_zoo_state::economy::account_transaction_types::TransactionKind;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRequest;
use openzt2_zoo_state::economy::general_economy_types::GeneralEconomyTransaction;
use openzt2_zoo_state::economy::money_types::Money;

use crate::construction::construction_payment_operation_types::ConstructionPaymentOperation;
use crate::construction::construction_payment_operation_types::ConstructionPaymentPurpose;

pub(super) fn create_construction_payment_request(
    commands: &mut Commands,
    construction_transaction: Entity,
    world_member: WorldMember,
    signed_cost: Money,
    payment_purpose: ConstructionPaymentPurpose,
    reverse_payment_direction: bool,
    general_transaction: Option<GeneralEconomyTransaction>,
) -> Option<TransactionRequest> {
    let absolute_amount = Money(signed_cost.0.checked_abs()?);
    let zoo_pays = (signed_cost.0 > 0) ^ reverse_payment_direction;
    let (debit_account, credit_account) = if zoo_pays {
        (Account::Zoo, Account::External)
    } else {
        (Account::External, Account::Zoo)
    };
    let mut payment_operation = commands.spawn((
        ConstructionPaymentOperation {
            construction_transaction,
            purpose: payment_purpose,
        },
        world_member,
    ));
    if let Some(general_transaction) = general_transaction {
        payment_operation.insert(general_transaction);
    }
    let payment_operation = payment_operation.id();
    Some(TransactionRequest {
        operation: payment_operation,
        debit: debit_account,
        credit: credit_account,
        amount: absolute_amount,
        kind: TransactionKind::Construction,
        subject: Some(construction_transaction),
    })
}
