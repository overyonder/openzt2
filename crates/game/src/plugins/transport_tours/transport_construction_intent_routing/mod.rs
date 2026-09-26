use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::placement::placement_transaction_types::ObjectPlacementCommitted;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    transport_circuit_types::{CircuitMember, TransportCircuit},
    transport_topology_types::{AttachTransportStationRequest, TransportStation},
};

/// Routes an ordinary committed station into the canonical circuit graph.
/// The source placement transaction remains the object owner; this system
/// emits only the transport mutation established by the authored station kind.
pub(super) fn attach_committed_transport_stations_to_compatible_or_new_circuits(
    mut committed_objects: MessageReader<ObjectPlacementCommitted>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    placed_stations: Query<(&TransportStation, &WorldMember)>,
    transport_circuits: Query<(Entity, &WorldMember), With<TransportCircuit>>,
    existing_station_members: Query<(&TransportStation, &CircuitMember)>,
    mut station_attachments: MessageWriter<AttachTransportStationRequest>,
    mut commands: Commands,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for committed_object in committed_objects.read() {
        let Ok((transport_station, station_world_member)) =
            placed_stations.get(committed_object.entity)
        else {
            continue;
        };
        let Some(station_definition) = world_definitions.find_station(transport_station.definition)
        else {
            continue;
        };
        let compatible_circuit = transport_circuits.iter().find_map(
            |(transport_circuit_entity, circuit_world_member)| {
                (circuit_world_member.root == station_world_member.root
                    && existing_station_members.iter().any(
                        |(existing_station, existing_member)| {
                            existing_member.0 == transport_circuit_entity
                                && world_definitions
                                    .find_station(existing_station.definition)
                                    .is_some_and(|existing_definition| {
                                        existing_definition.kind == station_definition.kind
                                    })
                        },
                    ))
                .then_some(transport_circuit_entity)
            },
        );
        let circuit = compatible_circuit.unwrap_or_else(|| {
            commands
                .spawn((
                    TransportCircuit {
                        definition: transport_station.definition,
                        closed: true,
                    },
                    *station_world_member,
                ))
                .id()
        });
        station_attachments.write(AttachTransportStationRequest {
            station: committed_object.entity,
            circuit,
        });
    }
}
