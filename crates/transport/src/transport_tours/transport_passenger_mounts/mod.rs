use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;

use openzt2_game_assets::{
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_zoo_state::{
    terrain::{
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
        terrain_world_sampling::{sample_terrain, terrain_chunk_at},
    },
    transport_tours::{transport_rider_types::*, transport_vehicle_types::TransportVehicle},
};

#[derive(Component)]
pub(super) struct VehicleSeatsHydrated;

#[derive(Component)]
pub(super) struct PendingSeatMount(AssetId);

pub(super) fn hydrate_vehicle_seats(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    catalogue: Res<WorldDefinitions>,
    vehicles: Query<(Entity, &TransportVehicle), Without<VehicleSeatsHydrated>>,
) {
    let Some(definitions) = catalogue.get(&definitions) else {
        return;
    };
    for (entity, vehicle) in &vehicles {
        for index in 0..vehicle.seats {
            let authored = definitions.find_vehicle_seat_by_index(vehicle.definition, index);
            let mut seat = commands.spawn((
                ChildOf(entity),
                TransportSeat {
                    index,
                    back_facing: authored.is_some_and(|seat| seat.back_facing),
                },
                Transform::IDENTITY,
            ));
            if let Some(node) = authored.and_then(|seat| seat.mount_node) {
                seat.insert(PendingSeatMount(node));
            }
        }
        commands.entity(entity).insert(VehicleSeatsHydrated);
    }
}

pub(super) fn resolve_authored_vehicle_seat_mounts(
    mut commands: Commands,
    mut seats: Query<(Entity, &ChildOf, &PendingSeatMount, &mut Transform), With<TransportSeat>>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    nodes: Query<
        (Option<&PrefabAuthoredAttachmentIdentifier>, Option<&Name>),
        Without<TransportSeat>,
    >,
    transforms: Query<&Transform, Without<TransportSeat>>,
) {
    for (seat, vehicle, mount, mut pose) in &mut seats {
        let vehicle = vehicle.parent();
        let Some(node) = children
            .iter_descendants_depth_first::<Children>(vehicle)
            .find(|&node| {
                nodes.get(node).is_ok_and(|(id, name)| {
                    id.is_some_and(|candidate| candidate.0 == mount.0)
                        || name.is_some_and(|name| {
                            AssetId::from_key(&name.as_str().to_ascii_lowercase()) == mount.0
                        })
                })
            })
        else {
            continue;
        };
        let mut local = Transform::IDENTITY;
        let mut complete = true;
        for entity in std::iter::once(node)
            .chain(parents.iter_ancestors::<ChildOf>(node))
            .take_while(|&entity| entity != vehicle)
        {
            let Ok(transform) = transforms.get(entity) else {
                complete = false;
                break;
            };
            local = transform.mul_transform(local);
        }
        if complete {
            pose.set_if_neq(local);
            commands
                .entity(seat)
                .insert(AuthoredTransportSeatPose)
                .remove::<PendingSeatMount>();
        }
    }
}

pub(super) fn project_seated_passenger_poses(
    mut riders: Query<(&TransportSeatRider, &mut Transform), With<TransportRider>>,
    seats: Query<
        (&ChildOf, &TransportSeat, &Transform),
        (With<AuthoredTransportSeatPose>, Without<TransportRider>),
    >,
    vehicles: Query<&Transform, (With<TransportVehicle>, Without<TransportRider>)>,
) {
    for (mount, mut rider) in &mut riders {
        let Ok((parent, seat, authored)) = seats.get(mount.0) else {
            continue;
        };
        let Ok(vehicle) = vehicles.get(parent.parent()) else {
            continue;
        };
        let mut local = *authored;
        if seat.back_facing {
            local.rotation *= Quat::from_rotation_y(std::f32::consts::PI);
        }
        rider.set_if_neq(vehicle.mul_transform(local));
    }
}

pub(super) fn eject_passengers_after_vehicle_charge(
    event: On<TransportVehicleCharged>,
    vehicles: Query<(&VehicleRiders, &Transform), With<TransportVehicle>>,
    mut guests: Query<&mut Transform, Without<TransportVehicle>>,
    terrain: Res<TerrainIndex>,
    assets: Res<Assets<TerrainAsset>>,
    chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    mut commands: Commands,
) {
    let Ok((riders, vehicle)) = vehicles.get(event.entity) else {
        return;
    };
    let position = vehicle.translation.xz();
    let ground = terrain_chunk_at(&terrain, position).and_then(|entity| {
        let (chunk, edited) = chunks.get(entity).ok()?;
        sample_terrain(chunk, assets.get(&chunk.asset)?, edited, position)
            .map(|sample| sample.height_m)
    });
    for rider in riders.iter() {
        if let (Some(height), Ok(mut pose)) = (ground, guests.get_mut(rider)) {
            pose.translation = Vec3::new(position.x, height, position.y);
        }
        commands.entity(rider).try_remove::<TransportRider>();
    }
}
