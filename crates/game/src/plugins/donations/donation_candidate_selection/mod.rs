use bevy::prelude::*;

use crate::plugins::{
    economy::facility_economy_types::Wallet,
    guests::guest_simulation_types::{Guest, GuestSatisfaction},
    locomotion::locomotion_types::{DockAt, NavAgent, NavigationRequestSequence, SpatialGrid},
};

use super::{
    donation_guest_decision_calculations::{
        calculate_eligible_donation_amount, create_donation_candidate_from_observation,
    },
    donation_guest_decision_types::{
        DonationCandidate, DonationObservation, DonationProgress, DonationRng,
    },
    donation_opportunity_types::{DonationAcceptor, DonationUsePoint},
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
        ),
        (
            With<Guest>,
            Changed<DonationObservation>,
            Without<DonationCandidate>,
            Without<DonationProgress>,
        ),
    >,
    acceptors: Query<(
        Entity,
        &DonationAcceptor,
        &DonationUsePoint,
        &GlobalTransform,
    )>,
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
    ) in &mut guests
    {
        let Some((acceptor, use_point)) = find_nearest_matching_donation_acceptor(
            &spatial_grid,
            guest_transform.translation(),
            *observation,
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
    acceptors: &Query<(
        Entity,
        &DonationAcceptor,
        &DonationUsePoint,
        &GlobalTransform,
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
    for (acceptor_entity, acceptor, use_point, acceptor_transform) in acceptors.iter() {
        if acceptor
            .beneficiary
            .is_some_and(|beneficiary| beneficiary != observation.subject)
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
