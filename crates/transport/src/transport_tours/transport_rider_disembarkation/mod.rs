use bevy::{
    ecs::entity::{EntityHashMap, EntityHashSet},
    prelude::*,
};

use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    behavior_task_execution_types::running_actions::AuthoredTourRide,
    economy::{
        account_transaction_types::{Account, TransactionKind, TransactionRequest},
        facility_economy_types::{Price, Wallet},
    },
    guests::guest_simulation_types::{
        Guest, GuestDeparturePoints, GuestReaction, GuestSatisfaction,
    },
    locomotion::locomotion_types::Arrived,
    transport_tours::{
        tour_scoring_types::{StationRatingPermille, TourScore, TransportTripCompleted},
        transport_circuit_types::{CircuitMember, TransportCircuit},
        transport_rider_types::{
            BoardedAtStation, TransportRider, TransportStationExit, VehicleRiders,
        },
        transport_topology_types::TransportStation,
        transport_vehicle_types::TransportVehicle,
    },
};

use crate::transport_tours::{
    tour_donation::TourDonation,
    tour_view_scoring::calculate_tour_rating_from_authored_score_ranges,
    tour_view_scoring::RideTourContributions,
    transport_fare_settlement::publish_transport_trip,
    transport_fare_types::{FareGuest, FareOperations, PendingTransportFare, QueuedFare},
};

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
        (With<Guest>, With<BoardedAtStation>),
    >,
    circuits: Query<&Price, With<TransportCircuit>>,
    donation: TourDonation,
    mut feedback: Query<
        (
            Option<&mut GuestSatisfaction>,
            Option<&mut GuestDeparturePoints>,
        ),
        With<Guest>,
    >,
    mut ratings: Query<&mut StationRatingPermille>,
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
    let mut rating_updates = EntityHashMap::default();
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
        let Ok(price) = circuits.get(member.0) else {
            continue;
        };
        if price.0 .0 < 0 {
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
        let mut last_rating = None;
        for guest in aboard.iter() {
            let Ok((score, world, paying, contributions, wallet, authored_ride)) =
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
            let rating = calculate_tour_rating_from_authored_score_ranges(catalogue, score.value);
            if let (Some((happiness, departure)), Ok((satisfaction, departure_points))) =
                (donation.ride_feedback(score.value), feedback.get_mut(guest))
            {
                if let Some(mut satisfaction) = satisfaction {
                    satisfaction.add_wellness_q16(happiness.saturating_neg().saturating_mul(10));
                }
                if let Some(mut points) = departure_points {
                    points.0 = points.0.saturating_add(departure);
                }
            }
            last_rating = Some(StationRatingPermille(
                (rating.clamp(0.0, 1.0) * 1000.0).round() as u16,
            ));
            let fare = PendingTransportFare {
                station,
                circuit: member.0,
                score: score.value,
                rating,
            };
            if price.0.is_positive() {
                let operation = commands
                    .spawn((*world, FareGuest(guest), fare, QueuedFare::pending(price.0)))
                    .id();
                transactions.write(TransactionRequest {
                    domain: openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Simulation,
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
        if let Some(rating) = last_rating {
            rating_updates.insert(station, rating);
        }
    }
    for (station, rating) in rating_updates {
        if let Ok(mut current) = ratings.get_mut(station) {
            current.set_if_neq(rating);
        } else {
            commands.entity(station).insert(rating);
        }
    }
}
