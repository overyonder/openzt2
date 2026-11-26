use bevy::{
    ecs::{entity::EntityHashSet, entity_disabling::Disabled, system::SystemParam},
    prelude::*,
};
use openzt2_zoo_state::donations::donation_guest_decision_types::{
    DonationCandidate, DonationProgress,
};

use openzt2_zoo_state::economy::facility_economy_types::Wallet;
use openzt2_zoo_state::guests::guest_simulation_types::Guest;
use openzt2_zoo_state::locomotion::locomotion_types::Arrived;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;

use super::{
    donation_guest_decision_calculations::{
        begin_donation_progress_from_candidate, find_invalid_donation_relation_rejection,
    },
    donation_opportunity_types::{DonationAcceptor, DonationOpportunity},
    donation_payment_types::{
        DonationPaymentRequested, DonationRejected, DonationRejection, DonationRequest,
    },
};

pub(super) fn begin_donation_interactions_after_guests_reach_acceptors(
    mut commands: Commands,
    mut arrivals: MessageReader<Arrived>,
    guests: Query<&DonationCandidate, With<Guest>>,
    acceptors: Query<(), With<DonationAcceptor>>,
    opportunities: Query<&DonationOpportunity>,
) {
    for arrival in arrivals.read() {
        let Ok(candidate) = guests.get(arrival.entity) else {
            continue;
        };
        if arrival.request_id != candidate.navigation_request_id
            || arrival.target != Some(candidate.acceptor)
            || acceptors.get(candidate.acceptor).is_err()
        {
            continue;
        }
        let Ok(opportunity) = opportunities.get(candidate.subject) else {
            continue;
        };

        commands
            .entity(arrival.entity)
            .remove::<DonationCandidate>()
            .insert(begin_donation_progress_from_candidate(
                *candidate,
                opportunity.interaction_ticks,
            ));
    }
}

pub(super) fn advance_donation_interactions_and_request_completed_payments(
    mut commands: Commands,
    mut guests: Query<
        (Entity, &mut DonationProgress, &Wallet),
        (With<Guest>, Without<DonationPaymentRequested>),
    >,
    acceptors: Query<(), With<DonationAcceptor>>,
    opportunities: Query<(), With<DonationOpportunity>>,
    mut payment_requests: MessageWriter<DonationRequest>,
    mut rejected_donations: MessageWriter<DonationRejected>,
) {
    for (guest, mut progress, wallet) in &mut guests {
        let rejection = find_invalid_donation_relation_rejection(
            acceptors.get(progress.acceptor).is_ok(),
            opportunities.get(progress.subject).is_ok(),
        )
        .or_else(|| (wallet.0 < progress.amount).then_some(DonationRejection::InsufficientFunds));
        if let Some(reason) = rejection {
            rejected_donations.write(DonationRejected {
                guest,
                acceptor: progress.acceptor,
                reason,
            });
            commands.entity(guest).remove::<DonationProgress>();
            continue;
        }
        if progress.remaining_ticks > 0 {
            progress.remaining_ticks -= 1;
            continue;
        }

        payment_requests.write(DonationRequest {
            guest,
            acceptor: progress.acceptor,
            subject: progress.subject,
            amount: progress.amount,
        });
        commands
            .entity(guest)
            .insert(DonationPaymentRequested { submitted: false });
    }
}

pub(super) fn reject_donation_candidates_after_navigation_failure(
    mut commands: Commands,
    mut navigation_failures: MessageReader<NavigationFailed>,
    guests: Query<&DonationCandidate, With<Guest>>,
    mut rejected_donations: MessageWriter<DonationRejected>,
) {
    for failure in navigation_failures.read() {
        let Ok(candidate) = guests.get(failure.entity) else {
            continue;
        };

        if failure.request_id != candidate.navigation_request_id {
            continue;
        }

        rejected_donations.write(DonationRejected {
            guest: failure.entity,
            acceptor: candidate.acceptor,
            reason: DonationRejection::Unreachable,
        });
        commands
            .entity(failure.entity)
            .remove::<DonationCandidate>();
    }
}

#[derive(Component)]
#[relationship(relationship_target = ArrivingDonors, allow_self_referential)]
pub(super) struct DonatingTo(Entity);

#[derive(Component)]
#[relationship_target(relationship = DonatingTo)]
pub(super) struct ArrivingDonors(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = InterestedDonors, allow_self_referential)]
pub(super) struct DonatingFor(Entity);

#[derive(Component)]
#[relationship_target(relationship = DonatingFor)]
pub(super) struct InterestedDonors(Vec<Entity>);

#[derive(SystemParam)]
pub(super) struct DonationRelationChanges<'w, 's> {
    new_phase: Query<
        'w,
        's,
        Entity,
        (
            With<Guest>,
            Or<(Changed<DonationCandidate>, Added<DonationProgress>)>,
        ),
    >,
    disabled: Query<
        'w,
        's,
        Entity,
        (
            Allow<Disabled>,
            Added<Disabled>,
            Or<(With<ArrivingDonors>, With<InterestedDonors>)>,
        ),
    >,
    donors: Query<'w, 's, &'static ArrivingDonors, Allow<Disabled>>,
    interested: Query<'w, 's, &'static InterestedDonors, Allow<Disabled>>,
    removed_candidate: RemovedComponents<'w, 's, DonationCandidate>,
    removed_progress: RemovedComponents<'w, 's, DonationProgress>,
    lost_acceptor_link: RemovedComponents<'w, 's, DonatingTo>,
    lost_subject_link: RemovedComponents<'w, 's, DonatingFor>,
    removed_acceptor: RemovedComponents<'w, 's, DonationAcceptor>,
    removed_opportunity: RemovedComponents<'w, 's, DonationOpportunity>,
    reenabled: RemovedComponents<'w, 's, Disabled>,
}

impl DonationRelationChanges<'_, '_> {
    fn affected(&mut self) -> EntityHashSet {
        let mut affected: EntityHashSet = self
            .new_phase
            .iter()
            .chain(self.removed_candidate.read())
            .chain(self.removed_progress.read())
            .chain(self.lost_acceptor_link.read())
            .chain(self.lost_subject_link.read())
            .chain(self.reenabled.read())
            .collect();
        for owner in self
            .disabled
            .iter()
            .chain(self.removed_acceptor.read())
            .chain(self.removed_opportunity.read())
        {
            if let Ok(donors) = self.donors.get(owner) {
                affected.extend(donors.iter());
            }
            if let Ok(donors) = self.interested.get(owner) {
                affected.extend(donors.iter());
            }
        }
        affected
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn reconcile_donation_phase_relationships(
    mut commands: Commands,
    mut changes: DonationRelationChanges,
    guests: Query<
        (
            Entity,
            Option<&DonationCandidate>,
            Option<&DonationProgress>,
            Option<&DonatingTo>,
            Option<&DonatingFor>,
        ),
        (
            With<Guest>,
            Or<(
                With<DonationCandidate>,
                With<DonationProgress>,
                With<DonatingTo>,
                With<DonatingFor>,
            )>,
        ),
    >,
    acceptors: Query<(), With<DonationAcceptor>>,
    opportunities: Query<(), With<DonationOpportunity>>,
    mut rejected_donations: MessageWriter<DonationRejected>,
) {
    for (guest, candidate, progress, acceptor_link, subject_link) in
        guests.iter_many(changes.affected()).matched()
    {
        let Some((acceptor, subject)) = candidate
            .map(|candidate| (candidate.acceptor, candidate.subject))
            .or_else(|| progress.map(|progress| (progress.acceptor, progress.subject)))
        else {
            commands.entity(guest).remove::<(DonatingTo, DonatingFor)>();
            continue;
        };

        let rejection = find_invalid_donation_relation_rejection(
            acceptors.get(acceptor).is_ok(),
            opportunities.get(subject).is_ok(),
        );
        if let Some(reason) = rejection {
            rejected_donations.write(DonationRejected {
                guest,
                acceptor,
                reason,
            });
            commands.entity(guest).remove::<(
                DonationCandidate,
                DonationProgress,
                DonationPaymentRequested,
                DonatingTo,
                DonatingFor,
            )>();
            continue;
        }
        if acceptor_link.is_none_or(|link| link.0 != acceptor) {
            commands.entity(guest).insert(DonatingTo(acceptor));
        }
        if subject_link.is_none_or(|link| link.0 != subject) {
            commands.entity(guest).insert(DonatingFor(subject));
        }
    }
}
