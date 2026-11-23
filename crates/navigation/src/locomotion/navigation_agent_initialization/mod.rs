use avian3d::prelude::RigidBody;
use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_zoo_state::locomotion::locomotion_types::ActiveTerrainDerivedNavigationGraph;
use openzt2_zoo_state::locomotion::locomotion_types::ContactSteering;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionCapacity;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::MotionProgress;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::locomotion_types::Steering;
use openzt2_zoo_state::locomotion::locomotion_types::Velocity;

pub(super) fn initialize_navigation_agents_with_bounded_runtime_state(
    mut commands: Commands,
    roots: Query<&LocomotionCapacity, With<ActiveTerrainDerivedNavigationGraph>>,
    navigation_agents: Query<
        (
            Entity,
            &WorldMember,
            &NavAgent,
            &GlobalTransform,
            Option<&RigidBody>,
        ),
        Without<Route>,
    >,
    mut navigation_failures: MessageWriter<NavigationFailed>,
) {
    for (
        navigation_agent_entity,
        member,
        navigation_agent,
        navigation_agent_transform,
        rigid_body,
    ) in &navigation_agents
    {
        if !roots
            .get(member.root)
            .is_ok_and(|capacity| capacity.is_valid())
        {
            continue;
        };
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
            Route::with_capacity(0),
            MotionProgress {
                last_position: navigation_agent_transform.translation(),
                stationary_ticks: 0,
            },
        ));
        // Entity owners select modes before navigation runtime state is ready.
        commands
            .entity(navigation_agent_entity)
            .insert_if_new(LocomotionMode::default());
        if rigid_body.is_none() {
            commands
                .entity(navigation_agent_entity)
                .insert(Velocity::default());
        }
    }
}
