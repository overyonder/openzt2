use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_zoo_state::guests::guest_simulation_types::Guest;

use crate::transport_tours::transport_relationship_cleanup;
use openzt2_zoo_state::transport_tours::tour_scoring_types::TourObservationMemory;
use openzt2_zoo_state::transport_tours::tour_scoring_types::TourScore;
use openzt2_zoo_state::transport_tours::transport_rider_types::TransportRider;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportStation;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::TransportVehicle;

#[test]
fn removing_transport_vehicle_clears_every_rider_relationship() {
    let mut application = App::new();
    transport_relationship_cleanup::install(&mut application);
    let vehicle_entity = application
        .world_mut()
        .spawn(TransportVehicle {
            definition: AssetId::from_key("vehicle"),
            seats: 2,

            maximum_speed_metres_per_second: 0.25,
        })
        .id();
    let station_entity = application
        .world_mut()
        .spawn(TransportStation {
            definition: AssetId::from_key("station"),
            capacity: 4,
        })
        .id();
    let guest_entity = application
        .world_mut()
        .spawn((
            Guest,
            TransportRider {
                vehicle: vehicle_entity,
            },
            openzt2_zoo_state::transport_tours::transport_rider_types::BoardedAtStation(
                station_entity,
            ),
            TourScore::default(),
            TourObservationMemory::default(),
            TourObservationDwell::default(),
        ))
        .id();
    application.update();
    application
        .world_mut()
        .entity_mut(vehicle_entity)
        .remove::<TransportVehicle>();
    application.update();
    assert!(application
        .world()
        .get::<TransportRider>(guest_entity)
        .is_none());
}
