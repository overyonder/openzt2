use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_zoo_state::locomotion::locomotion_types::ContactSteering;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::DirectLocomotion;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::locomotion_types::SpatialCell;
use openzt2_zoo_state::locomotion::locomotion_types::Steering;
use openzt2_zoo_state::locomotion::locomotion_types::MAX_STEERING_NEIGHBORS;
use openzt2_zoo_state::locomotion::locomotion_types::{
    SpatialCellIndex, SpatialGrid, SpatialGridMembership,
};

pub(super) fn calculate_route_following_and_separation_steering(
    spatial_grids: Query<&SpatialGrid>,
    spatial_membership: SpatialGridMembership,
    cell_indices: Query<&SpatialCellIndex>,
    mut route_following_navigation_agents: Query<
        (
            Entity,
            &NavAgent,
            &WorldMember,
            &GlobalTransform,
            Option<&SpatialCell>,
            &Route,
            &Destination,
            Option<&LocomotionMode>,
            &ContactSteering,
            &mut Steering,
        ),
        Without<DirectLocomotion>,
    >,
    neighboring_navigation_agents: Query<(&GlobalTransform, &NavAgent)>,
) {
    route_following_navigation_agents.par_iter_mut().for_each(
        |(
            navigation_agent_entity,
            navigation_agent,
            world,
            global_transform,
            spatial_cell,
            planned_route,
            _destination,
            locomotion_mode,
            contact_steering,
            mut steering,
        )| {
            let Some(current_route_target) = planned_route.current() else {
                steering.desired_velocity = Vec3::ZERO;
                return;
            };
            let navigation_agent_position = global_transform.translation();
            let mut route_target_offset = current_route_target - navigation_agent_position;
            if locomotion_mode.copied().unwrap_or_default() != LocomotionMode::Flight {
                route_target_offset.y = 0.0;
            }
            let mut desired_velocity =
                route_target_offset.normalize_or_zero() * navigation_agent.max_speed_mps;
            desired_velocity += contact_steering.direction
                * navigation_agent.max_speed_mps
                * contact_steering.factor;
            let mut visited_neighbor_count = 0;
            if let Some((spatial_grid, spatial_cell_coordinates)) = spatial_cell
                .and_then(|cell| cell_indices.get(cell.0).ok())
                .and_then(|index| {
                    let grid = spatial_grids.get(world.root).ok()?;
                    Some((grid, grid.cell_xy(index.0)?))
                })
            {
                'neighboring_cells: for neighboring_cell_z in
                    spatial_cell_coordinates.y.saturating_sub(1)
                        ..=(spatial_cell_coordinates.y + 1).min(spatial_grid.height - 1)
                {
                    for neighboring_cell_x in spatial_cell_coordinates.x.saturating_sub(1)
                        ..=(spatial_cell_coordinates.x + 1).min(spatial_grid.width - 1)
                    {
                        let neighboring_cell_index =
                            neighboring_cell_z * spatial_grid.width + neighboring_cell_x;
                        for neighboring_agent_entity in
                            spatial_membership.members(world.root, neighboring_cell_index)
                        {
                            if neighboring_agent_entity == navigation_agent_entity {
                                continue;
                            }
                            let Ok((neighboring_global_transform, neighboring_navigation_agent)) =
                                neighboring_navigation_agents.get(neighboring_agent_entity)
                            else {
                                continue;
                            };
                            let neighboring_agent_offset = navigation_agent_position
                                - neighboring_global_transform.translation();
                            let neighboring_agent_distance_squared =
                                neighboring_agent_offset.length_squared();
                            let required_separation_distance =
                                navigation_agent.radius_m + neighboring_navigation_agent.radius_m;
                            if neighboring_agent_distance_squared > 0.0001
                                && neighboring_agent_distance_squared
                                    < required_separation_distance * required_separation_distance
                            {
                                let neighboring_agent_distance =
                                    neighboring_agent_distance_squared.sqrt();
                                desired_velocity += neighboring_agent_offset
                                    / neighboring_agent_distance
                                    * navigation_agent.max_speed_mps
                                    * (1.0
                                        - neighboring_agent_distance
                                            / required_separation_distance);
                            }
                            visited_neighbor_count += 1;
                            if visited_neighbor_count == MAX_STEERING_NEIGHBORS {
                                break 'neighboring_cells;
                            }
                        }
                    }
                }
            }
            steering.desired_velocity =
                desired_velocity.clamp_length_max(navigation_agent.max_speed_mps);
        },
    );
}
