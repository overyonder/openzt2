use openzt2_game_data::AssetId;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitDirection;

#[derive(Debug, Clone, Copy)]
pub(super) struct TransportCircuitSnapshotRecord {
    pub persistent_identifier: PersistentId,
    pub transport_definition_identifier: AssetId,
    pub is_closed_loop: bool,
    pub is_running: bool,
    pub travel_direction: CircuitDirection,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TransportCircuitMemberSnapshotRecord {
    pub persistent_identifier: PersistentId,
    pub transport_definition_identifier: AssetId,
    pub circuit_persistent_identifier: PersistentId,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TransportVehicleSnapshotRecord {
    pub persistent_identifier: PersistentId,
    pub transport_definition_identifier: AssetId,
    pub circuit_persistent_identifier: PersistentId,
}

#[derive(Debug, Default)]
pub(super) struct TransportSnapshotRecords {
    pub circuit_records: Vec<TransportCircuitSnapshotRecord>,
    pub circuit_member_records: Vec<TransportCircuitMemberSnapshotRecord>,
    pub vehicle_records: Vec<TransportVehicleSnapshotRecord>,
    pub graph: TransportGraphSnapshot,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct TransportGraphSnapshot {
    pub poses: Vec<(u64, bevy::prelude::Transform)>,
    pub fares: Vec<(u64, i64)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fares_dollars: Vec<(u64, f32)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sky_station_crossings: Vec<SkyStationCrossingSnapshot>,
    pub junctions: Vec<(u64, Option<f32>)>,
    pub routes: Vec<TransportRouteSnapshot>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct SkyStationCrossingSnapshot {
    pub vehicle: u64,
    pub next_route: u64,
    pub reverse: bool,
    pub distance: f32,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct TransportRouteSnapshot {
    pub pieces: Vec<u64>,
    pub definition: AssetId,
    pub kind:
        openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind,
    pub maximum_grade_permille: u16,
    pub from: u64,
    pub from_socket: u8,
    pub to: u64,
    pub to_socket: u8,
    pub points: Vec<bevy::prelude::Vec3>,
    pub travel_points: Vec<bevy::prelude::Vec3>,
    pub length: f32,
}
