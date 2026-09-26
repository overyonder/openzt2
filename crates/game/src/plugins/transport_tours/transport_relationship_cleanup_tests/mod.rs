use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::guests::guest_simulation_types::Guest;

use super::{
    tour_scoring_types::{TourObservationCadence, TourObservationMemory, TourScore},
    transport_relationship_cleanup::remove_transport_relationships_broken_by_removed_circuits_stations_tracks_or_vehicles,
    transport_rider_types::TransportRider,
    transport_topology_types::TransportStation,
    transport_vehicle_types::TransportVehicle,
};

#[test]
fn removing_transport_vehicle_clears_every_rider_relationship() {
    let mut application = App::new();
    application.add_systems(
        Update,
        remove_transport_relationships_broken_by_removed_circuits_stations_tracks_or_vehicles,
    );
    let vehicle_entity = application
        .world_mut()
        .spawn(TransportVehicle {
            definition: AssetId::from_key("vehicle"),
            seats: 2,
            occupied: 1,
            maximum_speed_metres_per_second: 0.25,
        })
        .id();
    let station_entity = application
        .world_mut()
        .spawn(TransportStation {
            definition: AssetId::from_key("station"),
            capacity: 4,
            queued: 0,
            occupied: 0,
        })
        .id();
    let guest_entity = application
        .world_mut()
        .spawn((
            Guest,
            TransportRider {
                vehicle: vehicle_entity,
                boarded_station: station_entity,
                seat_index: 0,
            },
            TourScore::default(),
            TourObservationMemory::default(),
            TourObservationCadence::default(),
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
