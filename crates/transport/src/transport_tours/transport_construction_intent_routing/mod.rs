use bevy::prelude::*;
use openzt2_zoo_state::economy::{facility_economy_types::Price, money_types::Money};
use openzt2_zoo_state::transport_tours::authored_station_fare::station_fare;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementCommitted;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;

use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitMember;
use openzt2_zoo_state::transport_tours::transport_circuit_types::TransportCircuit;
use openzt2_zoo_state::transport_tours::transport_topology_types::AttachTransportStationRequest;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportStation;

/// Routes an ordinary committed station into the canonical circuit graph.
/// The source placement transaction remains the object owner; this system
/// emits only the transport mutation established by the authored station kind.
pub(super) fn create_circuits_for_new_transport_stations(
    mut committed_objects: MessageReader<ObjectPlacementCommitted>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    placed_stations: Query<
        (&TransportStation, &WorldMember, Option<&Price>),
        Without<CircuitMember>,
    >,
    mut station_attachments: MessageWriter<AttachTransportStationRequest>,
    mut ids: Query<&mut PersistentIdAllocator>,
    mut commands: Commands,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let mut assigned = bevy::ecs::entity::EntityHashSet::default();
    for committed_object in committed_objects.read() {
        if assigned.contains(&committed_object.entity) {
            continue;
        }
        let Ok((transport_station, station_world_member, fare)) =
            placed_stations.get(committed_object.entity)
        else {
            continue;
        };
        if world_definitions
            .find_station(transport_station.definition)
            .is_none()
        {
            continue;
        }
        let Some(id) = ids
            .get_mut(station_world_member.root)
            .ok()
            .and_then(|mut allocator| allocator.allocate().ok())
        else {
            continue;
        };
        let circuit = commands
            .spawn((
                id,
                DefinitionId(transport_station.definition),
                fare.copied().unwrap_or_else(|| {
                    Price(
                        station_fare(world_definitions, transport_station.definition, None)
                            .unwrap_or(Money::ZERO),
                    )
                }),
                TransportCircuit {
                    definition: transport_station.definition,
                    closed: true,
                },
                *station_world_member,
            ))
            .id();
        assigned.insert(committed_object.entity);
        station_attachments.write(AttachTransportStationRequest {
            station: committed_object.entity,
            circuit,
        });
    }
}
