use avian3d::prelude::RigidBody;
use bevy::prelude::*;

use super::locomotion_types::{
    ActiveTerrainDerivedNavigationGraph, ContactSteering, LocomotionCapacity, LocomotionMode,
    MotionProgress, NavAgent, NavigationFailed, NavigationFailure, Route, SpatialCell, Steering,
    Velocity,
};

pub(super) fn initialize_navigation_agents_with_bounded_runtime_state(
    mut commands: Commands,
    active_navigation_graph: Option<Res<ActiveTerrainDerivedNavigationGraph>>,
    locomotion_capacity: Option<Res<LocomotionCapacity>>,
    navigation_agents: Query<
        (Entity, &NavAgent, &GlobalTransform, Option<&RigidBody>),
        Without<Route>,
    >,
    mut navigation_failures: MessageWriter<NavigationFailed>,
) {
    let Some(locomotion_capacity) =
        locomotion_capacity.filter(|locomotion_capacity| locomotion_capacity.is_valid())
    else {
        return;
    };
    let Some(_active_navigation_graph) = active_navigation_graph else {
        return;
    };
    for (navigation_agent_entity, navigation_agent, navigation_agent_transform, rigid_body) in
        &navigation_agents
    {
        if !navigation_agent.is_valid() {
            navigation_failures.write(NavigationFailed {
                entity: navigation_agent_entity,
                request_id: 0,
                reason: NavigationFailure::NoRoute,
            });
            continue;
        }
        commands.entity(navigation_agent_entity).insert((
            Steering::default(),
            ContactSteering::default(),
            Route::with_capacity(locomotion_capacity.max_route_points),
            SpatialCell::OUTSIDE,
            LocomotionMode::default(),
            MotionProgress {
                last_position: navigation_agent_transform.translation(),
                stationary_ticks: 0,
            },
        ));
        if rigid_body.is_none() {
            commands
                .entity(navigation_agent_entity)
                .insert(Velocity::default());
        }
    }
}
