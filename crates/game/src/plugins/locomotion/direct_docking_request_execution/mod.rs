use bevy::prelude::*;

use super::locomotion_types::{
    Destination, DockAt, Docking, NavAgent, NavigationFailed, NavigationFailure,
};

pub(super) fn accept_direct_docking_requests(
    mut commands: Commands,
    mut docking_requests: MessageReader<DockAt>,
    navigation_agents: Query<(), With<NavAgent>>,
    docking_target_transforms: Query<&GlobalTransform>,
    mut navigation_failures: MessageWriter<NavigationFailed>,
) {
    for docking_request in docking_requests.read() {
        if navigation_agents.get(docking_request.entity).is_err() {
            navigation_failures.write(NavigationFailed {
                entity: docking_request.entity,
                request_id: docking_request.request_id,
                reason: NavigationFailure::NoRoute,
            });
            continue;
        }
        let Ok(docking_target_transform) = docking_target_transforms.get(docking_request.target)
        else {
            navigation_failures.write(NavigationFailed {
                entity: docking_request.entity,
                request_id: docking_request.request_id,
                reason: NavigationFailure::TargetGone,
            });
            continue;
        };
        if !docking_request.local_point.is_finite()
            || !docking_request.local_forward.is_finite()
            || !docking_request.radius_m.is_finite()
            || docking_request.radius_m < 0.0
        {
            navigation_failures.write(NavigationFailed {
                entity: docking_request.entity,
                request_id: docking_request.request_id,
                reason: NavigationFailure::OutsideWorld,
            });
            continue;
        }
        let docking_world_point =
            docking_target_transform.transform_point(docking_request.local_point);
        let docking_world_forward = docking_target_transform
            .affine()
            .transform_vector3(docking_request.local_forward)
            .normalize_or_zero();
        commands.entity(docking_request.entity).insert((
            Docking {
                request_id: docking_request.request_id,
                target: docking_request.target,
                point: docking_world_point,
                forward: docking_world_forward,
                radius_m: docking_request.radius_m,
            },
            Destination {
                request_id: docking_request.request_id,
                world: docking_world_point,
                arrival_radius_m: docking_request.radius_m,
            },
        ));
    }
}
