use bevy::prelude::*;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationVehicleSeatDefinition;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;

use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_zoo_state::transport_tours::{
    transport_rider_types::*, transport_vehicle_types::TransportVehicle,
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
    let cabin_tag = AssetId::from_key("passenger_cabin");
    for (entity, vehicle) in &vehicles {
        let cabin_mounts = definitions
            .find_vehicle(vehicle.definition)
            .and_then(|vehicle| definitions.find_object(vehicle.object))
            .into_iter()
            .flat_map(|object| &object.interaction_slots)
            .filter(|slot| slot.reservation_tag == cabin_tag && !slot.is_queue)
            .map(|slot| {
                (!slot.target_node_name.is_empty())
                    .then(|| AssetId::from_key(&slot.target_node_name.to_ascii_lowercase()))
            });
        for (index, cabin_mount) in
            (0..vehicle.seats).zip(cabin_mounts.chain(std::iter::repeat(None)))
        {
            let authored = definitions.find_vehicle_seat_by_index(vehicle.definition, index);
            let mut seat = commands.spawn((
                ChildOf(entity),
                TransportSeat {
                    index,
                    back_facing: authored.is_some_and(|seat| seat.back_facing),
                },
                Transform::IDENTITY,
            ));
            if let Some(node) = authored.and_then(|seat| seat.mount_node).or(cabin_mount) {
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
        rider.set_if_neq(occupied_seat_pose(vehicle, authored, seat.back_facing));
    }
}

pub fn occupied_seat_pose(vehicle: &Transform, mount: &Transform, back_facing: bool) -> Transform {
    let mut local = *mount;
    if back_facing {
        local.rotation *= Quat::from_rotation_y(std::f32::consts::PI);
    }
    vehicle.mul_transform(local)
}

fn seat_view_bearing(pose: &Transform, target: Vec3) -> f32 {
    let local = pose.rotation.inverse() * (target - pose.translation);
    local.x.atan2(local.z).to_degrees()
}

fn seat_bearing_is_within_band(bearing: f32, limits: [i16; 4], active: u8) -> bool {
    (0..2).any(|index| {
        let left = index == 0;
        let angle = if left { bearing } else { -bearing };
        let first = 2 * index;
        let required = 0b11 << first;
        active & required == required
            && angle >= f32::from(limits[first])
            && angle <= f32::from(limits[first + 1])
    })
}

pub(super) fn seat_can_view_target(
    definition: &TransportationVehicleSeatDefinition,
    occupied_pose: &Transform,
    target: Vec3,
) -> bool {
    seat_bearing_is_within_band(
        seat_view_bearing(occupied_pose, target),
        definition.view_limits_degrees,
        0b1111,
    )
}

/// Prefix used by the seated guest's authored directional animation action.
pub fn seat_view_animation_prefix<'a>(
    definition: &'a TransportationVehicleSeatDefinition,
    occupied_pose: &Transform,
    target: Vec3,
) -> Option<&'a str> {
    let bearing = seat_view_bearing(occupied_pose, target);
    let offset = target - occupied_pose.translation;
    let vertical = offset.xz().length().atan2(offset.y).to_degrees();
    definition.animation_states.iter().find_map(|state| {
        (state.up_degrees.unwrap_or(0.0) < vertical
            && vertical <= state.down_degrees.unwrap_or(180.0)
            && seat_bearing_is_within_band(
                bearing,
                state.angle_limits_degrees,
                state.present_limits,
            ))
        .then_some(state.clip_prefix.as_str())
    })
}
