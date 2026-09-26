use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::{
    guests::guest_simulation_types::Guest,
    locomotion::locomotion_types::{Arrived, NavAgent, NavFlags},
};

use super::{
    transport_circuit_types::CircuitMember,
    transport_rider_boarding::board_waiting_guests_when_transport_vehicle_arrives,
    transport_rider_types::{TransportRider, WaitingForTransport},
    transport_topology_types::TransportStation,
    transport_vehicle_types::{StationDestination, TransportVehicle},
};

fn transport_station_for_boarding_test() -> TransportStation {
    TransportStation {
        definition: AssetId::from_key("station"),
        capacity: 8,
        queued: 2,
        occupied: 0,
    }
}

fn one_seat_transport_vehicle_for_boarding_test() -> TransportVehicle {
    TransportVehicle {
        definition: AssetId::from_key("vehicle"),
        seats: 1,
        occupied: 0,
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
fn boarding_uses_stable_entity_order_and_updates_station_vehicle_and_guest_relations() {
    let mut application = App::new();
    application
        .add_message::<Arrived>()
        .add_systems(Update, board_waiting_guests_when_transport_vehicle_arrives);

    let transport_circuit_entity = application.world_mut().spawn_empty().id();
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
    let first_guest_entity = application
        .world_mut()
        .spawn((
            Guest,
            guest_navigation_agent_for_boarding_test(),
            WaitingForTransport {
                station: station_entity,
                circuit: transport_circuit_entity,
            },
        ))
        .id();
    let second_guest_entity = application
        .world_mut()
        .spawn((
            Guest,
            guest_navigation_agent_for_boarding_test(),
            WaitingForTransport {
                station: station_entity,
                circuit: transport_circuit_entity,
            },
        ))
        .id();
    application.world_mut().write_message(Arrived {
        entity: vehicle_entity,
        request_id: 0,
        target: Some(station_entity),
    });
    application.update();

    let boarded_guest_entity = if first_guest_entity.to_bits() < second_guest_entity.to_bits() {
        first_guest_entity
    } else {
        second_guest_entity
    };
    let waiting_guest_entity = if boarded_guest_entity == first_guest_entity {
        second_guest_entity
    } else {
        first_guest_entity
    };
    assert_eq!(
        application
            .world()
            .get::<TransportRider>(boarded_guest_entity),
        Some(&TransportRider {
            vehicle: vehicle_entity,
            boarded_station: station_entity,
            seat_index: 0,
        }),
    );
    assert!(application
        .world()
        .get::<WaitingForTransport>(waiting_guest_entity)
        .is_some());
    assert_eq!(
        application
            .world()
            .get::<TransportVehicle>(vehicle_entity)
            .unwrap()
            .occupied,
        1,
    );
    assert_eq!(
        application
            .world()
            .get::<TransportStation>(station_entity)
            .unwrap()
            .queued,
        1,
    );
}
