use bevy::{platform::collections::HashSet, prelude::*};

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::economy::account_transaction_types::Account;
use crate::plugins::economy::account_transaction_types::TransactionCompleted;
use crate::plugins::economy::account_transaction_types::TransactionKind;
use crate::plugins::economy::account_transaction_types::TransactionRejected;
use crate::plugins::economy::account_transaction_types::TransactionRequest;
use crate::plugins::simulation_time::simulation_clock_types::ZooDayAdvanced;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::staff_employment_types::{Employment, PendingWagePayment, Staff};

pub(in crate::plugins::staff) fn request_staff_wage_payments_on_first_day_of_month(
    mut commands: Commands,
    mut days: MessageReader<ZooDayAdvanced>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    staff: Query<(Entity, &Employment, &WorldMember), With<Staff>>,
    pending: Query<&PendingWagePayment>,
    mut transactions: MessageWriter<TransactionRequest>,
    mut requested_now: Local<HashSet<(Entity, u64)>>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    requested_now.clear();
    let ticks_per_day = catalogue.timing().ticks_per_day;
    for day in days.read() {
        if day.calendar.day != 1 {
            continue;
        }
        let month_ordinal = u32::from(day.calendar.year)
            .saturating_mul(12)
            .saturating_add(u32::from(day.calendar.month.saturating_sub(1)));
        let due_tick = u64::from(day.current_day).saturating_mul(u64::from(ticks_per_day));
        for (staff_entity, employment, member) in &staff {
            if employment.wage.0 <= 0
                || employment.hired_month_ordinal >= month_ordinal
                || pending.iter().any(|operation| {
                    operation.staff == staff_entity && operation.due_tick == due_tick
                })
                || !requested_now.insert((staff_entity, due_tick))
            {
                continue;
            }
            let operation = commands
                .spawn((
                    PendingWagePayment {
                        staff: staff_entity,
                        due_tick,
                    },
                    *member,
                ))
                .id();
            transactions.write(TransactionRequest {
                operation,
                debit: Account::Zoo,
                credit: Account::External,
                amount: employment.wage,
                kind: TransactionKind::Wage,
                subject: Some(staff_entity),
            });
        }
    }
}

pub(in crate::plugins::staff) fn discard_settled_staff_wage_payment_operations(
    mut commands: Commands,
    mut completed: MessageReader<TransactionCompleted>,
    mut rejected: MessageReader<TransactionRejected>,
    pending: Query<(Entity, &PendingWagePayment)>,
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
