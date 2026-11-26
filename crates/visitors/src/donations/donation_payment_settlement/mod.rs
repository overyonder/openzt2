use openzt2_zoo_state::donations::donation_guest_decision_types::DonationProgress;
use std::collections::BTreeMap;

use bevy::prelude::*;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::behavior_task_execution_types::{
    running_actions::{
        mark_behavior_task_for_failure_and_stop_navigation, RunningBehaviorInstruction,
    },
    BehaviorTaskExecutionState,
};
use openzt2_zoo_state::{
    animal_lifecycle::types::SpeciesHandle,
    progression::fame_types::{GuestSpeciesFameViews, ZooSpeciesFameViews},
    shows::show_stage_types::ShowStage,
    transport_tours::transport_circuit_types::TransportCircuit,
};

use openzt2_zoo_state::economy::account_transaction_types::TransactionCompleted;
use openzt2_zoo_state::economy::account_transaction_types::TransactionKind;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRejected;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRejection;
use openzt2_zoo_state::guests::guest_simulation_types::Guest;

use super::{
    donation_opportunity_types::DonationOpportunity,
    donation_payment_calculations::{
        accumulate_completed_donation_total, completed_transaction_matches_pending_donation_payment,
    },
    donation_payment_types::{
        DonationCategoryTotals, DonationCompleted, DonationPaymentRequested, DonationPaymentState,
        DonationRejected, DonationRejection, DonationTotal, PendingDonationPayment,
    },
};

pub(super) fn donation_timeout_token_name() -> openzt2_game_data::AssetId {
    openzt2_game_data::AssetId::from_key("DonationTokenTimeout")
}

pub(super) fn settle_completed_donation_transactions_and_update_acceptor_totals(
    mut commands: Commands,
    mut completed_transactions: MessageReader<TransactionCompleted>,
    mut payment_operations: Query<(&PendingDonationPayment, &mut DonationPaymentState)>,
    guests: Query<&WorldMember, (With<Guest>, With<DonationPaymentRequested>)>,
    donation_opportunities: Query<&DonationOpportunity>,
    species: Query<&SpeciesHandle>,
    attributed_subjects: Query<(), Or<(With<ShowStage>, With<TransportCircuit>)>>,
    (mut fame_views, mut zoo_views): (
        Query<&mut GuestSpeciesFameViews>,
        Query<&mut ZooSpeciesFameViews>,
    ),
    mut donation_totals: Query<&mut DonationTotal>,
    mut donation_category_totals: Query<&mut DonationCategoryTotals>,
    mut world_deltas: super::zoo_donation_summary_presentation::DonationWorldDeltas,
    mut completed_donations: MessageWriter<DonationCompleted>,
    mut rejected_donations: MessageWriter<DonationRejected>,
    clock: Option<Single<&openzt2_world_state::simulation_time::simulation_clock_types::ZooClock>>,
    definitions: Option<Res<openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions>>,
    definition_assets: Option<Res<Assets<openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset>>>,
) {
    let timeout = (|| {
        let clock = clock.as_deref()?;
        let definitions = definitions.as_deref()?.get(definition_assets.as_deref()?)?;
        let duration_ns = definitions.show_scheduling()?.donation_token_timeout_ns;
        let timestep = definitions.simulation_timestep()?;
        let ticks = u128::from(duration_ns).div_ceil(timestep.as_nanos());
        (ticks > 0).then(|| {
            (
                clock.tick,
                clock
                    .tick
                    .saturating_add(u64::try_from(ticks).unwrap_or(u64::MAX)),
            )
        })
    })();
    // Missing components are inserted after all payments in this run are counted.
    let mut new_donation_totals: BTreeMap<Entity, DonationTotal> = BTreeMap::new();
    let mut new_category_totals: BTreeMap<Entity, DonationCategoryTotals> = BTreeMap::new();
    for transaction in completed_transactions.read() {
        if transaction.kind != TransactionKind::Donation {
            continue;
        }
        let Ok((pending_payment, mut payment_state)) =
            payment_operations.get_mut(transaction.operation)
        else {
            continue;
        };
        if *payment_state != DonationPaymentState::Pending {
            continue;
        }
        *payment_state = DonationPaymentState::Resolved;

        let transaction_matches_payment = completed_transaction_matches_pending_donation_payment(
            *pending_payment,
            transaction.debit,
            transaction.credit,
            transaction.amount,
            transaction.subject,
        );
        if !transaction_matches_payment || guests.get(pending_payment.guest).is_err() {
            reject_pending_donation_payment_and_remove_operation(
                &mut commands,
                &mut rejected_donations,
                *pending_payment,
                transaction.operation,
                DonationRejection::AccountFailure,
            );
            continue;
        }

        let Ok(donation_opportunity) = donation_opportunities.get(pending_payment.subject) else {
            reject_pending_donation_payment_and_remove_operation(
                &mut commands,
                &mut rejected_donations,
                *pending_payment,
                transaction.operation,
                DonationRejection::InvalidSubject,
            );
            continue;
        };
        let updated_total = match donation_totals.get_mut(pending_payment.acceptor) {
            Ok(current_total) => {
                accumulate_completed_donation_total(*current_total, pending_payment.amount)
            }
            Err(_) => match new_donation_totals.get(&pending_payment.acceptor) {
                Some(total) => accumulate_completed_donation_total(*total, pending_payment.amount),
                None => Some(DonationTotal {
                    amount: pending_payment.amount,
                    count: 1,
                }),
            },
        };
        let Some(updated_total) = updated_total else {
            reject_pending_donation_payment_and_remove_operation(
                &mut commands,
                &mut rejected_donations,
                *pending_payment,
                transaction.operation,
                DonationRejection::AccountFailure,
            );
            continue;
        };

        let category_total_was_updated =
            if let Ok(mut totals) = donation_category_totals.get_mut(pending_payment.acceptor) {
                totals
                    .accumulate_completed_donation(
                        donation_opportunity.category,
                        pending_payment.amount,
                    )
                    .is_some()
            } else {
                new_category_totals
                    .entry(pending_payment.acceptor)
                    .or_default()
                    .accumulate_completed_donation(
                        donation_opportunity.category,
                        pending_payment.amount,
                    )
                    .is_some()
            };
        if !category_total_was_updated {
            reject_pending_donation_payment_and_remove_operation(
                &mut commands,
                &mut rejected_donations,
                *pending_payment,
                transaction.operation,
                DonationRejection::AccountFailure,
            );
            continue;
        }
        world_deltas.record(
            pending_payment.acceptor,
            donation_opportunity.category,
            pending_payment.amount,
        );

        if let Ok(mut total) = donation_totals.get_mut(pending_payment.acceptor) {
            *total = updated_total;
        } else {
            new_donation_totals.insert(pending_payment.acceptor, updated_total);
        }
        if let (Ok(species), Ok(mut views), Ok(guest)) = (
            species.get(pending_payment.subject),
            fame_views.get_mut(pending_payment.guest),
            guests.get(pending_payment.guest),
        ) {
            let view = views.0.entry(species.species).or_default();
            let previous = *view;
            view.donation += pending_payment.amount.dollars();
            if let Ok(mut zoo) = zoo_views.get_mut(guest.root) {
                zoo.replace_contribution(species.species, previous, *view);
            }
        }
        if pending_payment.subject != pending_payment.acceptor
            && attributed_subjects.contains(pending_payment.subject)
        {
            if let Ok(mut current) = donation_totals.get_mut(pending_payment.subject) {
                if let Some(total) =
                    accumulate_completed_donation_total(*current, pending_payment.amount)
                {
                    *current = total;
                }
            } else {
                let current = new_donation_totals
                    .entry(pending_payment.subject)
                    .or_default();
                if let Some(total) =
                    accumulate_completed_donation_total(*current, pending_payment.amount)
                {
                    *current = total;
                }
            }
        }
        if let Some((available_at, expires_at)) = timeout {
            commands.spawn((
                ChildOf(pending_payment.guest),
                openzt2_zoo_state::animal_behavior::behavior_token_types::BehaviorSelfToken {
                    name: donation_timeout_token_name(),
                    available_at,
                    expires_at,
                    payload: Some(pending_payment.acceptor),
                    retain_on_failure: true,
                },
            ));
        }
        clear_guest_donation_payment_state(&mut commands, pending_payment.guest);
        commands.entity(transaction.operation).despawn();
        completed_donations.write(DonationCompleted {
            guest: pending_payment.guest,
            acceptor: pending_payment.acceptor,
            subject: pending_payment.subject,
            amount: pending_payment.amount,
        });
    }
    for (acceptor, total) in new_donation_totals {
        commands.entity(acceptor).insert(total);
    }
    for (acceptor, totals) in new_category_totals {
        commands.entity(acceptor).insert(totals);
    }
}

pub(super) fn settle_rejected_donation_transactions_and_clear_guest_payment_state(
    mut commands: Commands,
    mut rejected_transactions: MessageReader<TransactionRejected>,
    mut payment_operations: Query<(&PendingDonationPayment, &mut DonationPaymentState)>,
    mut rejected_donations: MessageWriter<DonationRejected>,
) {
    for transaction in rejected_transactions.read() {
        if transaction.kind != TransactionKind::Donation {
            continue;
        }
        let Ok((pending_payment, mut payment_state)) =
            payment_operations.get_mut(transaction.operation)
        else {
            continue;
        };
        if *payment_state != DonationPaymentState::Pending {
            continue;
        }
        *payment_state = DonationPaymentState::Resolved;

        let transaction_matches_payment = completed_transaction_matches_pending_donation_payment(
            *pending_payment,
            transaction.debit,
            transaction.credit,
            transaction.amount,
            transaction.subject,
        );
        let rejection = if transaction_matches_payment {
            map_economy_transaction_rejection_to_donation_rejection(transaction.reason)
        } else {
            DonationRejection::AccountFailure
        };
        reject_pending_donation_payment_and_remove_operation(
            &mut commands,
            &mut rejected_donations,
            *pending_payment,
            transaction.operation,
            rejection,
        );
    }
}

fn reject_pending_donation_payment_and_remove_operation(
    commands: &mut Commands,
    rejected_donations: &mut MessageWriter<DonationRejected>,
    pending_payment: PendingDonationPayment,
    payment_operation: Entity,
    reason: DonationRejection,
) {
    rejected_donations.write(DonationRejected {
        guest: pending_payment.guest,
        acceptor: pending_payment.acceptor,
        reason,
    });
    clear_guest_donation_payment_state(commands, pending_payment.guest);
    commands.entity(payment_operation).despawn();
}

fn clear_guest_donation_payment_state(commands: &mut Commands, guest: Entity) {
    if let Ok(mut guest_entity) = commands.get_entity(guest) {
        guest_entity
            .remove::<DonationProgress>()
            .remove::<DonationPaymentRequested>();
    }
}

const fn map_economy_transaction_rejection_to_donation_rejection(
    transaction_rejection: TransactionRejection,
) -> DonationRejection {
    match transaction_rejection {
        TransactionRejection::InsufficientFunds => DonationRejection::InsufficientFunds,
        // Cancellation did not commit a donation; use the ordinary account
        // rejection cleanup without misreporting insufficient guest funds.
        TransactionRejection::Cancelled
        | TransactionRejection::NonPositive
        | TransactionRejection::MissingAccount
        | TransactionRejection::Overflow => DonationRejection::AccountFailure,
    }
}

pub(super) fn resume_tour_donation_after_payment(
    mut commands: Commands,
    mut completed: MessageReader<TransactionCompleted>,
    mut rejected: MessageReader<TransactionRejected>,
    operations: Query<(
        &PendingDonationPayment,
        &TourDonationInstruction,
        &WorldMember,
    )>,
    actors: Query<(&BehaviorTaskExecutionState, &RunningBehaviorInstruction)>,
    clocks: Query<&ZooClock>,
) {
    let outcomes = completed
        .read()
        .map(|result| (result.operation, true))
        .chain(rejected.read().map(|result| (result.operation, false)));
    for (operation, accepted) in outcomes {
        let Ok((payment, owner, member)) = operations.get(operation) else {
            continue;
        };
        let Ok((task, instruction)) = actors.get(payment.guest) else {
            continue;
        };
        if instruction.id() != owner.0 || !instruction.is_current(task) {
            continue;
        }
        if accepted {
            if let Ok(clock) = clocks.get(member.root) {
                instruction.complete(payment.guest, clock.tick, &mut commands);
            }
        } else {
            mark_behavior_task_for_failure_and_stop_navigation(payment.guest, &mut commands);
        }
    }
}
