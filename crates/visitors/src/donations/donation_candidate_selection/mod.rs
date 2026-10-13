use bevy::prelude::*;

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::PendingBehaviorSet;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::economy::facility_economy_types::Wallet;
use openzt2_zoo_state::economy::service_types::ServiceReservation;
use openzt2_zoo_state::guests::guest_simulation_types::Guest;
use openzt2_zoo_state::guests::guest_simulation_types::GuestPhase;
use openzt2_zoo_state::guests::guest_simulation_types::GuestSatisfaction;
use openzt2_zoo_state::guests::guest_simulation_types::Viewing;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::DockAt;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationRequestSequence;
use openzt2_zoo_state::locomotion::locomotion_types::SpatialGrid;

use super::{
    donation_guest_decision_calculations::{
        calculate_eligible_donation_amount, create_donation_candidate_from_observation,
    },
    donation_guest_decision_types::{
        DonationCandidate, DonationObservation, DonationProgress, DonationRng,
    },
    donation_opportunity_types::{DonationAcceptor, DonationOpportunity, DonationUsePoint},
    donation_payment_types::DonationPaymentRequested,
};

pub(super) fn choose_donation_candidates_from_new_guest_observations(
    mut commands: Commands,
    spatial_grid: Res<SpatialGrid>,
    mut guests: Query<
        (
            Entity,
            &GlobalTransform,
            &GuestSatisfaction,
            &Wallet,
            &NavAgent,
            &DonationObservation,
            &mut DonationRng,
            &Viewing,
            &GuestPhase,
            &WorldMember,
        ),
        (
            With<Guest>,
            Changed<DonationObservation>,
            Without<DonationCandidate>,
            Without<DonationProgress>,
            Without<DonationPaymentRequested>,
            Without<Destination>,
            Without<BehaviorTaskExecutionState>,
            Without<PendingBehaviorSet>,
            Without<ServiceReservation>,
        ),
    >,
    acceptors: Query<(
        Entity,
        &DonationAcceptor,
        &DonationUsePoint,
        &GlobalTransform,
        &WorldMember,
    )>,
    subjects: Query<&WorldMember, With<DonationOpportunity>>,
    mut navigation_request_sequence: ResMut<NavigationRequestSequence>,
    mut docking_requests: MessageWriter<DockAt>,
) {
    for (
        guest,
        guest_transform,
        satisfaction,
        wallet,
        navigation_agent,
        observation,
        mut donation_rng,
        viewing,
        phase,
        world,
    ) in &mut guests
    {
        if *phase != GuestPhase::Visiting
            || *viewing != observation.viewing
            || !subjects
                .get(viewing.subject)
                .is_ok_and(|subject| subject.root == world.root)
        {
            continue;
        }
        let Some((acceptor, use_point)) = find_nearest_matching_donation_acceptor(
            &spatial_grid,
            guest_transform.translation(),
            *observation,
            world.root,
            &acceptors,
        ) else {
            continue;
        };

        let probability_roll = (observation.chance_permyriad < 10_000)
            .then(|| donation_rng.0.range_u32(10_000))
            .flatten();
        let Some(amount) = calculate_eligible_donation_amount(
            satisfaction.0,
            wallet.0,
            *observation,
            probability_roll,
        ) else {
            continue;
        };

        let navigation_request_id = navigation_request_sequence.next();
        commands
            .entity(guest)
            .insert(create_donation_candidate_from_observation(
                *observation,
                acceptor,
                amount,
                navigation_request_id,
            ));
        docking_requests.write(DockAt {
            entity: guest,
            request_id: navigation_request_id,
            target: acceptor,
            local_point: use_point.local_point,
            local_forward: use_point.local_forward,
            radius_m: navigation_agent.radius_m,
        });
    }
}

fn find_nearest_matching_donation_acceptor(
    spatial_grid: &SpatialGrid,
    guest_position: Vec3,
    observation: DonationObservation,
    world: Entity,
    acceptors: &Query<(
        Entity,
        &DonationAcceptor,
        &DonationUsePoint,
        &GlobalTransform,
        &WorldMember,
    )>,
) -> Option<(Entity, DonationUsePoint)> {
    if !spatial_grid.cell_size_m.is_finite() || spatial_grid.cell_size_m <= 0.0 {
        return None;
    }

    let maximum_distance = spatial_grid.cell_size_m * f32::from(observation.acceptor_radius_cells);
    let maximum_distance_squared = maximum_distance * maximum_distance;
    let mut nearest_acceptor: Option<(f32, u64, Entity, DonationUsePoint)> = None;

    // Donation acceptors are static world objects, not locomotion agents, so
    // they deliberately do not live in the agent-only avoidance grid. This
    // query runs only when a guest receives a new viewing observation.
    for (acceptor_entity, acceptor, use_point, acceptor_transform, member) in acceptors.iter() {
        if member.root != world {
            continue;
        }
        if acceptor
            .beneficiary
            .is_some_and(|beneficiary| beneficiary != observation.viewing.subject)
        {
            continue;
        }
        if !use_point.local_point.is_finite() || !use_point.local_forward.is_finite() {
            continue;
        }

        let world_use_point = acceptor_transform.transform_point(use_point.local_point);
        let distance_squared = guest_position.distance_squared(world_use_point);
        if distance_squared > maximum_distance_squared {
            continue;
        }

        let acceptor_bits = acceptor_entity.to_bits();
        if nearest_acceptor.is_none_or(|current| {
            distance_squared < current.0
                || (distance_squared == current.0 && acceptor_bits < current.1)
        }) {
            nearest_acceptor = Some((distance_squared, acceptor_bits, acceptor_entity, *use_point));
        }
    }

    nearest_acceptor.map(|(_, _, acceptor, use_point)| (acceptor, use_point))
}
