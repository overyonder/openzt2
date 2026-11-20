use bevy::ecs::entity::EntityIndexMap;
use bevy::prelude::*;

use openzt2_zoo_state::locomotion::locomotion_types::DockAt;
use openzt2_zoo_state::locomotion::locomotion_types::Docking;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure;
use openzt2_zoo_state::locomotion::locomotion_types::{Destination, EntityPursuit};

pub(super) fn accept_direct_docking_requests(
    mut commands: Commands,
    mut docking_requests: MessageReader<DockAt>,
    mut navigation_agents: Query<(Option<&mut Docking>, Option<&mut Destination>), With<NavAgent>>,
    docking_target_transforms: Query<&GlobalTransform>,
    mut navigation_failures: MessageWriter<NavigationFailed>,
) {
    let mut inserted_destinations = EntityIndexMap::<Destination>::default();
    let mut inserted_docking = EntityIndexMap::<Docking>::default();
    for docking_request in docking_requests.read() {
        let Ok((docking, destination)) = navigation_agents.get_mut(docking_request.entity) else {
            navigation_failures.write(NavigationFailed {
                entity: docking_request.entity,
                request_id: docking_request.request_id,
                reason: NavigationFailure::NoRoute,
            });
            continue;
        };
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
        let next_docking = Docking {
            request_id: docking_request.request_id,
            target: docking_request.target,
            point: docking_world_point,
            forward: docking_world_forward,
            radius_m: docking_request.radius_m,
        };
        let next_destination = Destination {
            request_id: docking_request.request_id,
            world: docking_world_point,
            arrival_radius_m: docking_request.radius_m,
        };
        let mut actor = commands.entity(docking_request.entity);
        actor.remove::<EntityPursuit>();
        if let Some(mut docking) = docking {
            docking.set_if_neq(next_docking);
        } else {
            inserted_docking.insert(docking_request.entity, next_docking);
        }
        if let Some(mut destination) = destination {
            destination.set_if_neq(next_destination);
        } else {
            inserted_destinations.insert(docking_request.entity, next_destination);
        }
    }
    for (actor, destination) in inserted_destinations {
        commands.entity(actor).insert(destination);
    }
    for (actor, docking) in inserted_docking {
        commands.entity(actor).insert(docking);
    }
}
