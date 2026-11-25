use bevy::{
    ecs::entity::{EntityHashMap, EntityHashSet},
    prelude::*,
};
use openzt2_world_state::world_spawn::{
    persistent_id_types::{PersistentId, PersistentIdAllocator},
    world_membership_types::{DefinitionId, WorldMember},
};
use openzt2_zoo_state::{
    economy::{facility_economy_types::Price, money_types::Money},
    transport_tours::{
        transport_circuit_types::{
            CircuitDirection, CircuitMember, CircuitMembers, TransportCircuit,
        },
        transport_topology_types::{
            TrackIncidence, TrackRoute, TransportStation, TransportTrackJunction,
        },
        transport_vehicle_types::VehicleOnTrack,
    },
};

use super::topology::TransportEdgesDisconnected;

pub(super) fn split_disconnected_transport_circuits(
    mut changes: MessageReader<TransportEdgesDisconnected>,
    circuits: Query<(
        &TransportCircuit,
        &CircuitMembers,
        &WorldMember,
        &CircuitDirection,
        Option<&Price>,
    )>,
    endpoints: Query<
        (Option<&PersistentId>, Option<&TransportStation>),
        Or<(With<TransportStation>, With<TransportTrackJunction>)>,
    >,
    routes: Query<(&CircuitMember, TrackRoute)>,
    vehicles: Query<&VehicleOnTrack>,
    incidence: TrackIncidence,
    mut allocators: Query<&mut PersistentIdAllocator>,
    mut commands: Commands,
) {
    let dirty: EntityHashSet = changes.read().map(|change| change.0).collect();
    for circuit in dirty {
        let Ok((current, members, world, direction, price)) = circuits.get(circuit) else {
            continue;
        };
        let mut starts = members
            .iter()
            .filter_map(|entity| {
                endpoints
                    .get(entity)
                    .ok()
                    .map(|(id, station)| (entity, id, station))
            })
            .collect::<Vec<_>>();
        starts.sort_unstable_by_key(|(entity, id, station)| {
            (
                station.is_none(),
                id.map_or(u64::MAX, |id| id.0),
                entity.to_bits(),
            )
        });
        let mut owner = EntityHashMap::<Entity>::default();
        let mut seen = EntityHashSet::default();
        let mut preserved = false;
        for (start, _, station) in starts {
            if !seen.insert(start) {
                continue;
            }
            let mut group = vec![start];
            let mut pending = vec![start];
            let mut first_route = None;
            while let Some(endpoint) = pending.pop() {
                for track in incidence.incident(endpoint) {
                    let Ok((member, route)) = routes.get(track) else {
                        continue;
                    };
                    if member.0 != circuit || !seen.insert(track) {
                        continue;
                    }
                    first_route.get_or_insert(route.definition);
                    group.push(track);
                    for neighbour in [route.from(), route.to()] {
                        if seen.insert(neighbour) {
                            pending.push(neighbour);
                            group.push(neighbour);
                        }
                    }
                }
            }
            if station.is_none() && first_route.is_none() {
                // A gap may leave an unused endpoint for an undo. It has no ride.
                commands.entity(start).remove::<CircuitMember>();
                continue;
            }
            let group_circuit = if !preserved {
                preserved = true;
                circuit
            } else {
                let Ok(mut ids) = allocators.get_mut(world.root) else {
                    continue;
                };
                let Ok(id) = ids.allocate() else { continue };
                let definition = station
                    .map(|station| station.definition)
                    .or(first_route)
                    .unwrap_or(current.definition);
                commands
                    .spawn((
                        id,
                        DefinitionId(definition),
                        TransportCircuit {
                            definition,
                            closed: true,
                        },
                        *direction,
                        price.copied().unwrap_or(Price(Money::ZERO)),
                        *world,
                    ))
                    .id()
            };
            for entity in group {
                owner.insert(entity, group_circuit);
                if group_circuit != circuit {
                    commands.entity(entity).insert(CircuitMember(group_circuit));
                }
            }
        }
        for member in members.iter() {
            let Ok(vehicle) = vehicles.get(member) else {
                continue;
            };
            if let Some(&next) = owner.get(&vehicle.0).filter(|&&next| next != circuit) {
                commands.entity(member).insert(CircuitMember(next));
            }
        }
    }
}
