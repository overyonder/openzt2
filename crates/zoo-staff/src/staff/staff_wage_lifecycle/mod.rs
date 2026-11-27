use bevy::prelude::*;

use openzt2_world_state::simulation_time::simulation_clock_types::ZooDayAdvanced;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::economy::account_transaction_types::Account;
use openzt2_zoo_state::economy::account_transaction_types::TransactionCompleted;
use openzt2_zoo_state::economy::account_transaction_types::TransactionKind;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRejected;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRequest;

use super::staff_employment_types::{Employment, Staff};

#[derive(Component, Default)]
pub(super) struct LastRequestedWage(Option<(Entity, u32)>);

#[derive(Component)]
pub(super) struct WagePaymentOperation;

pub(super) fn request_monthly_wages(
    mut commands: Commands,
    mut days: MessageReader<ZooDayAdvanced>,
    mut staff: Query<(Entity, &Employment, &WorldMember, &mut LastRequestedWage), With<Staff>>,
    mut transactions: MessageWriter<TransactionRequest>,
) {
    for day in days.read().filter(|day| day.calendar.day == 1) {
        let month_ordinal = u32::from(day.calendar.year)
            .saturating_mul(12)
            .saturating_add(u32::from(day.calendar.month.saturating_sub(1)));
        for (employee, employment, member, mut last_request) in &mut staff {
            if member.root != day.root
                || !employment.wage.is_positive()
                || employment.hired_month_ordinal >= month_ordinal
                || last_request
                    .0
                    .is_some_and(|(root, date)| root == day.root && date >= day.current_day)
            {
                continue;
            }
            last_request.0 = Some((day.root, day.current_day));
            let operation = commands.spawn((WagePaymentOperation, *member)).id();
            transactions.write(TransactionRequest {
                domain: openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Simulation,
                operation,
                debit: Account::Zoo,
                credit: Account::External,
                amount: employment.wage,
                kind: TransactionKind::Wage,
                subject: Some(employee),
            });
        }
    }
}

pub(super) fn discard_settled_staff_wage_payment_operations(
    mut commands: Commands,
    mut completed: MessageReader<TransactionCompleted>,
    mut rejected: MessageReader<TransactionRejected>,
    pending: Query<(), With<WagePaymentOperation>>,
) {
    for result in completed.read() {
        if pending.get(result.operation).is_ok() {
            commands.entity(result.operation).despawn();
        }
    }
    for result in rejected.read() {
        if pending.get(result.operation).is_ok() {
            commands.entity(result.operation).despawn();
        }
    }
}
