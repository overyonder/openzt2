use openzt2_game_data::AssetId;

use crate::plugins::{
    transport_tours::transport_circuit_types::CircuitDirection,
    world_spawn::persistent_id_types::PersistentId,
};

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
}
