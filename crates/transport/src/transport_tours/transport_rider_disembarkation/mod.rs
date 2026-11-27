use std::collections::VecDeque;

use bevy::{
    ecs::{entity::EntityHashSet, relationship::RelationshipTarget},
    prelude::*,
};

use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentId, world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    behavior_task_execution_types::running_actions::AuthoredTourRide,
    economy::{
        account_transaction_types::{
            Account, TransactionDomain, TransactionKind, TransactionRequest,
        },
        facility_economy_types::{Price, Wallet},
        money_types::Money,
    },
    guests::guest_simulation_types::{
        Guest, GuestDeparturePoints, GuestReaction, GuestSatisfaction,
    },
    locomotion::locomotion_types::Arrived,
    transport_tours::{
        tour_scoring_types::{StationRatingPermille, TourScore, TransportTripCompleted},
        transport_circuit_types::{CircuitMember, CircuitMembers, TransportCircuit},
        transport_rider_types::{TransportRider, TransportStationExit, VehicleRiders},
        transport_topology_types::{TrackSegment, TransportStation},
        transport_vehicle_types::TransportVehicle,
    },
};

use crate::transport_tours::{
    tour_donation::TourDonation,
    tour_view_scoring::RideTourContributions,
    tour_view_scoring::{calculate_tour_rating_from_authored_score_ranges, normalized_tour_rating},
    transport_fare_settlement::publish_transport_trip,
    transport_fare_types::{FareGuest, FareOperations, PendingTransportFare, QueuedFare},
    transport_route_position_calculations::TrackTravelDistances,
};

/// Most recent authored rating samples for rides completed at a station.
#[derive(Component, Default)]
pub struct StationTourRatingHistory(pub VecDeque<f32>);

pub(super) fn disembark_transport_riders_and_request_service_fare_transactions(
    mut arrivals: MessageReader<Arrived>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    vehicles: Query<(&CircuitMember, &VehicleRiders, &Transform), With<TransportVehicle>>,
    stations: Query<(&CircuitMember, &WorldMember, Option<&Children>), With<TransportStation>>,
    exits: Query<&GlobalTransform, With<TransportStationExit>>,
    mut poses: Query<&mut Transform, (With<TransportRider>, Without<TransportVehicle>)>,
    riders: Query<
        (
            &TourScore,
            &WorldMember,
            Has<FareOperations>,
            Option<&RideTourContributions>,
            Option<&Wallet>,
            Option<&AuthoredTourRide>,
        ),
        (
            With<Guest>,
            With<openzt2_zoo_state::transport_tours::transport_rider_types::BoardedAtStation>,
        ),
    >,
    (circuits, tracks): (
        Query<(&Price, Option<&CircuitMembers>), With<TransportCircuit>>,
        Query<(&TrackSegment, &TrackTravelDistances, Option<&PersistentId>)>,
    ),
    donation: TourDonation,
    mut feedback: Query<
        (
            Option<&mut GuestSatisfaction>,
            Option<&mut GuestDeparturePoints>,
        ),
        With<Guest>,
    >,
    mut ratings: Query<(&mut StationRatingPermille, &mut StationTourRatingHistory)>,
    mut transactions: MessageWriter<TransactionRequest>,
    mut trips: MessageWriter<TransportTripCompleted>,
    mut reactions: MessageWriter<GuestReaction>,
    mut commands: Commands,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        arrivals.clear();
        return;
    };
    let mut disembarked = EntityHashSet::new();
    let policy = catalogue.tour_scoring();
    for arrival in arrivals.read() {
        let Some(station) = arrival.target else {
            continue;
        };
        let (
            Ok((member, aboard, vehicle_pose)),
            Ok((station_circuit, station_world, exits_at_station)),
        ) = (vehicles.get(arrival.entity), stations.get(station))
        else {
            continue;
        };
        if member.0 != station_circuit.0 {
            continue;
        }
        let Ok((price, circuit_members)) = circuits.get(member.0) else {
            continue;
        };
        if price.0 < Money::ZERO {
            continue;
        }
        let exit = exits_at_station
            .into_iter()
            .flat_map(|children| children.iter())
            .filter_map(|child| exits.get(child).ok())
            .min_by(|left, right| {
                vehicle_pose
                    .translation
                    .distance_squared(left.translation())
                    .total_cmp(
                        &vehicle_pose
                            .translation
                            .distance_squared(right.translation()),
                    )
            });
        let mut circuit_length = None;
        for guest in aboard.iter() {
            let Ok((views, world, paying, contributions, wallet, authored_ride)) =
                riders.get(guest)
            else {
                continue;
            };
            if paying
                || world.root != station_world.root
                || authored_ride.is_some_and(|ride| ride.destination != station)
                || !disembarked.insert(guest)
            {
                continue;
            }
            let distance = *circuit_length.get_or_insert_with(|| {
                let mut lengths: Vec<_> = circuit_members
                    .into_iter()
                    .flat_map(RelationshipTarget::iter)
                    .filter_map(|entity| {
                        let (track, distances, id) = tracks.get(entity).ok()?;
                        Some((
                            id.map_or(u64::MAX, |id| id.0),
                            entity,
                            distances.travel_length(track),
                        ))
                    })
                    .collect();
                lengths.sort_unstable_by_key(|&(id, entity, _)| (id, entity));
                lengths
                    .into_iter()
                    .map(|(_, _, length)| length)
                    .sum::<f32>()
            });
            let score = distance
                + contributions
                    .filter(|_| policy.is_some())
                    .map_or(views.value, |contributions| {
                        contributions.0[0].max(0.0) + contributions.0[1].max(0.0)
                    });
            let ranges = policy.map_or(&[][..], |policy| policy.rating_ranges.as_slice());
            let (sample, limits) = calculate_tour_rating_from_authored_score_ranges(ranges, score);
            let rating = normalized_tour_rating(sample, limits);
            if let (Some(policy), Ok((mut station_rating, mut history))) =
                (policy, ratings.get_mut(station))
            {
                history.0.push_back(sample);
                let limit = usize::from(policy.tour_history);
                let excess = history.0.len().saturating_sub(limit);
                history.0.drain(..excess);
                let average = if history.0.is_empty() {
                    limits[0]
                } else {
                    history.0.iter().sum::<f32>() / history.0.len() as f32
                };
                station_rating.set_if_neq(StationRatingPermille(
                    (normalized_tour_rating(average, limits) * 1000.0).round() as u16,
                ));
            }
            if let (Some((happiness, departure)), Ok((satisfaction, departure_points))) =
                (donation.ride_feedback(sample), feedback.get_mut(guest))
            {
                if let Some(mut satisfaction) = satisfaction {
                    satisfaction.add_wellness_q16(happiness.saturating_neg().saturating_mul(10));
                }
                if let Some(mut points) = departure_points {
                    points.0 = points.0.saturating_add(departure).max(0);
                }
            }
            let fare = PendingTransportFare {
                station,
                circuit: member.0,
                score,
                rating,
            };
            if price.0.is_positive() {
                let operation = commands
                    .spawn((*world, FareGuest(guest), fare, QueuedFare::pending(price.0)))
                    .id();
                transactions.write(TransactionRequest {
                    domain: TransactionDomain::Simulation,
                    operation,
                    debit: Account::Entity(guest),
                    credit: Account::Zoo,
                    amount: price.0,
                    kind: TransactionKind::Service,
                    subject: Some(member.0),
                });
            } else {
                publish_transport_trip(guest, &fare, &mut trips, &mut reactions);
            }
            if let (Some(contributions), Some(budget)) = (
                contributions,
                wallet.and_then(|wallet| wallet.0.checked_sub(price.0)),
            ) {
                donation.offer(
                    &mut commands,
                    (guest, *world, budget),
                    member.0,
                    contributions.0,
                );
            }
            if let (Some(exit), Ok(mut pose)) = (exit, poses.get_mut(guest)) {
                pose.translation = exit.translation();
                pose.rotation = exit.rotation();
            }
            commands.entity(guest).remove::<TransportRider>();
        }
    }
}
