use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::economy::general_economy_types::GeneralEconomyLedger;

use super::{
    account_transaction_types::RecordedAccountMovement,
    account_transaction_types::{TransactionKind, TransactionRejection},
    guest_admission_types::AdmissionPrice,
    money_types::Money,
    scenario_economy_command_types::{ScenarioEconomyCommand, ScenarioEconomyOperation},
    zoo_cash_types::{UnlimitedZooCash, ZooCash},
};

#[derive(SystemParam)]
pub struct ScenarioEconomyAccess<'w, 's> {
    worlds: Query<
        'w,
        's,
        (
            &'static mut AdmissionPrice,
            &'static mut ZooCash,
            Option<&'static UnlimitedZooCash>,
            Option<&'static mut GeneralEconomyLedger>,
        ),
        With<WorldRoot>,
    >,
    movements: MessageWriter<'w, RecordedAccountMovement<false>>,
}

impl ScenarioEconomyAccess<'_, '_> {
    /// Applies an operation against the selected zoo before the next script action.
    pub fn apply(
        &mut self,
        root: Entity,
        operation: ScenarioEconomyOperation,
    ) -> Result<Money, TransactionRejection> {
        let (mut admission, mut cash, unlimited, general) = self
            .worlds
            .get_mut(root)
            .map_err(|_| TransactionRejection::MissingAccount)?;
        let delta = match operation {
            ScenarioEconomyOperation::GrantCash(amount) => amount,
            ScenarioEconomyOperation::TakeCash(amount) => -amount,
            ScenarioEconomyOperation::SetAdmission(amount) => {
                admission.set_if_neq(AdmissionPrice::from_adult(amount));
                return Ok(cash.0);
            }
        };
        if delta == Money::ZERO {
            return Ok(cash.0);
        }
        let next_cash = if delta < Money::ZERO && unlimited.is_some_and(|policy| policy.0) {
            cash.0
        } else {
            if delta < Money::ZERO && cash.0 < delta.abs() {
                return Err(TransactionRejection::InsufficientFunds);
            }
            cash.0
                .checked_add(delta)
                .ok_or(TransactionRejection::Overflow)?
        };
        if let Some(mut ledger) = general.filter(|ledger| ledger.cash().is_some()) {
            ledger
                .record_completed_transaction(delta)
                .ok_or(TransactionRejection::Overflow)?;
        }
        cash.set_if_neq(ZooCash(next_cash));
        self.movements.write(RecordedAccountMovement {
            root,
            kind: TransactionKind::Reward,
            subject: None,
            zoo_delta: delta,
            named_payment: None,
        });
        Ok(next_cash)
    }
}

pub(super) fn apply_scenario_cash_and_admission_commands(
    world_members: Query<&WorldMember>,
    mut commands: MessageReader<ScenarioEconomyCommand>,
    mut economy: ScenarioEconomyAccess,
) {
    for command in commands.read() {
        let root = world_members
            .get(command.terminal)
            .map_or(command.terminal, |member| member.root);
        let _ = economy.apply(root, command.operation);
    }
}
