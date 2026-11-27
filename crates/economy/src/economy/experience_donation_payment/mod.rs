use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    animal_behavior::behavior_token_types::{BehaviorSelfToken, ClaimedBehaviorSelfToken},
    behavior_task_execution_types::{
        running_actions::{
            mark_behavior_task_for_failure_and_stop_navigation, BehaviorInstructionId,
            RunBehaviorEconomy, RunningBehaviorInstruction, WhenBehaviorAction,
        },
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    },
    donations::{
        donation_opportunity_types::{DonationAcceptor, DonationOpportunity},
        donation_payment_types::{
            DonationPaymentRequested, DonationPaymentState, PendingDonationPayment,
        },
    },
    economy::{
        account_transaction_types::{
            Account, TransactionCompleted, TransactionKind, TransactionRejected, TransactionRequest,
        },
        experience_donation::ExperienceDonation,
    },
    guests::guest_simulation_types::Guest,
    shows::show_stage_types::ShowStage,
};

#[derive(Component)]
pub(super) struct ShowDonationInstruction(BehaviorInstructionId);

pub(super) fn request_experience_donation_payments(
    activation: On<WhenBehaviorAction<RunBehaviorEconomy>>,
    mut commands: Commands,
    guests: Query<
        (
            Entity,
            &WorldMember,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorEconomy,
        ),
        (
            With<Guest>,
            Without<PendingBehaviorTaskFailure>,
            Without<DonationPaymentRequested>,
        ),
    >,
    mut tokens: Query<(
        &ChildOf,
        &BehaviorSelfToken,
        &ClaimedBehaviorSelfToken,
        &mut ExperienceDonation,
    )>,
    subjects: Query<&WorldMember, (With<DonationOpportunity>, With<ShowStage>)>,
    boxes: Query<&WorldMember, With<DonationAcceptor>>,
    mut payments: MessageWriter<TransactionRequest>,
) {
    let donate = AssetId::from_key("donate");
    let Ok((guest, world, task, instruction, economy)) = guests.get(activation.entity) else {
        return;
    };
    if instruction.id() != activation.instruction || !instruction.is_current(task) {
        return;
    }
    let (Some(token), Some(acceptor)) = (task.object, task.target) else {
        return;
    };
    let Ok((owner, token, claim, mut donation)) = tokens.get_mut(token) else {
        return;
    };
    if owner.parent() != guest || claim.execution_id != task.execution_id || donation.submitted {
        return;
    }

    if token.name != AssetId::from_key("t_donationshow") || economy.transaction != donate {
        return;
    }
    if !subjects
        .get(donation.subject)
        .is_ok_and(|member| member.root == world.root)
        || !boxes
            .get(acceptor)
            .is_ok_and(|member| member.root == world.root)
        || !donation.amount.is_positive()
    {
        mark_behavior_task_for_failure_and_stop_navigation(guest, &mut commands);
        return;
    }
    donation.submitted = true;
    commands
        .entity(guest)
        .insert(DonationPaymentRequested { submitted: true });
    let operation = commands.spawn((
        *world,
        PendingDonationPayment {
            guest,
            acceptor,
            subject: donation.subject,
            amount: donation.amount,
        },
        DonationPaymentState::Pending,
        ShowDonationInstruction(instruction.id()),
    ));
    payments.write(TransactionRequest {
        domain:
            openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Simulation,
        operation: operation.id(),
        debit: Account::Entity(guest),
        credit: Account::Zoo,
        amount: donation.amount,
        kind: TransactionKind::Donation,
        subject: Some(acceptor),
    });
}

pub(super) fn resume_show_donation_instructions_after_payment(
    mut commands: Commands,
    clocks: Query<&ZooClock>,
    mut completed: MessageReader<TransactionCompleted>,
    mut rejected: MessageReader<TransactionRejected>,
    payments: Query<(
        &PendingDonationPayment,
        &ShowDonationInstruction,
        &WorldMember,
    )>,
    actors: Query<
        (&BehaviorTaskExecutionState, &RunningBehaviorInstruction),
        With<RunBehaviorEconomy>,
    >,
) {
    let results = completed
        .read()
        .map(|result| (result.operation, true))
        .chain(rejected.read().map(|result| (result.operation, false)));
    for (operation, accepted) in results {
        let Ok((payment, owner, member)) = payments.get(operation) else {
            continue;
        };
        let Ok((task, instruction)) = actors.get(payment.guest) else {
            continue;
        };
        if instruction.id() == owner.0 && instruction.is_current(task) {
            if accepted {
                let Ok(clock) = clocks.get(member.root) else {
                    continue;
                };
                instruction.complete(payment.guest, clock.tick, &mut commands);
            } else {
                mark_behavior_task_for_failure_and_stop_navigation(payment.guest, &mut commands);
            }
        }
    }
}
