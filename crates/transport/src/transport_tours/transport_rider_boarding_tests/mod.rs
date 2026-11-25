use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::transport_tours::transport_rider_types::{
    StationQueue, TransportSeat, TransportSeatRider, VehicleRiders,
};

use openzt2_zoo_state::guests::guest_simulation_types::Guest;
use openzt2_zoo_state::locomotion::locomotion_types::Arrived;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavFlags;

use crate::transport_tours::transport_rider_boarding::board_waiting_guests_when_transport_vehicle_arrives;
use openzt2_zoo_state::transport_tours::transport_rider_types::TransportRider;
use openzt2_zoo_state::transport_tours::transport_rider_types::WaitingForTransport;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportStation;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::StationDestination;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::TransportVehicle;
use openzt2_zoo_state::{
    economy::{
        facility_economy_types::{Price, Wallet},
        money_types::Money,
    },
    transport_tours::transport_circuit_types::{CircuitMember, TransportCircuit},
};

fn transport_station_for_boarding_test() -> TransportStation {
    TransportStation {
        definition: AssetId::from_key("station"),
        capacity: 8,
    }
}

fn one_seat_transport_vehicle_for_boarding_test() -> TransportVehicle {
    TransportVehicle {
        definition: AssetId::from_key("vehicle"),
        seats: 1,

        maximum_speed_metres_per_second: 0.25,
    }
}

fn guest_navigation_agent_for_boarding_test() -> NavAgent {
    NavAgent {
        radius_m: 0.5,
        max_speed_mps: 1.0,
        acceleration_mps2: 1.0,
        capabilities: NavFlags::GUEST,
    }
}

#[test]
fn boarding_uses_reservation_order_and_updates_station_vehicle_and_guest_relations() {
    let mut application = App::new();
    application
        .add_message::<Arrived>()
        .add_systems(Update, board_waiting_guests_when_transport_vehicle_arrives);

    let transport_circuit_entity = application
        .world_mut()
        .spawn((
            TransportCircuit {
                definition: AssetId::from_key("groundtrack"),
                closed: false,
            },
            Price(Money(100)),
        ))
        .id();
    let station_entity = application
        .world_mut()
        .spawn((
            transport_station_for_boarding_test(),
            CircuitMember(transport_circuit_entity),
        ))
        .id();
    let vehicle_entity = application
        .world_mut()
        .spawn((
            one_seat_transport_vehicle_for_boarding_test(),
            CircuitMember(transport_circuit_entity),
            StationDestination(station_entity),
        ))
        .id();
    let seat = application
        .world_mut()
        .spawn((
            ChildOf(vehicle_entity),
            TransportSeat {
                index: 0,
                back_facing: false,
            },
        ))
        .id();
    let first_guest_entity = application
        .world_mut()
        .spawn((
            Guest,
            guest_navigation_agent_for_boarding_test(),
            Wallet(Money(200)),
        ))
        .id();
    let second_guest_entity = application
        .world_mut()
        .spawn((
            Guest,
            guest_navigation_agent_for_boarding_test(),
            Wallet(Money(200)),
        ))
        .id();
    application
        .world_mut()
        .entity_mut(second_guest_entity)
        .insert(WaitingForTransport {
            station: station_entity,
        });
    application
        .world_mut()
        .entity_mut(first_guest_entity)
        .insert(WaitingForTransport {
            station: station_entity,
        });
    application.world_mut().write_message(Arrived {
        entity: vehicle_entity,
        request_id: 0,
        target: Some(station_entity),
    });
    application.update();

    let boarded_guest_entity = second_guest_entity;
    let waiting_guest_entity = first_guest_entity;
    assert_eq!(
        application
            .world()
            .get::<TransportRider>(boarded_guest_entity),
        Some(&TransportRider {
            vehicle: vehicle_entity,
        }),
    );
    assert_eq!(
        application
            .world()
            .get::<TransportSeatRider>(boarded_guest_entity)
            .map(|mount| mount.0),
        Some(seat),
    );
    assert!(application
        .world()
        .get::<WaitingForTransport>(waiting_guest_entity)
        .is_some());
    assert_eq!(
        application
            .world()
            .get::<VehicleRiders>(vehicle_entity)
            .unwrap()
            .len(),
        1,
    );
    assert_eq!(
        application
            .world()
            .get::<StationQueue>(station_entity)
            .unwrap()
            .len(),
        1,
    );
}
