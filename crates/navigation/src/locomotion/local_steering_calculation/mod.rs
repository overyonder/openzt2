use bevy::prelude::*;

use openzt2_zoo_state::locomotion::locomotion_types::ContactSteering;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::DirectLocomotion;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::locomotion_types::SpatialCell;
use openzt2_zoo_state::locomotion::locomotion_types::SpatialGrid;
use openzt2_zoo_state::locomotion::locomotion_types::Steering;
use openzt2_zoo_state::locomotion::locomotion_types::MAX_STEERING_NEIGHBORS;

/// The largest agent radius separation looks out for.
const LARGEST_NEIGHBOR_RADIUS_M: f32 = 3.0;

pub(super) fn calculate_route_following_and_separation_steering(
    spatial_grid: Res<SpatialGrid>,
    mut route_following_navigation_agents: Query<
        (
            Entity,
            &NavAgent,
            &GlobalTransform,
            &SpatialCell,
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
    for (
        navigation_agent_entity,
        navigation_agent,
        global_transform,
        spatial_cell,
        planned_route,
        _destination,
        locomotion_mode,
        contact_steering,
        mut steering,
    ) in &mut route_following_navigation_agents
    {
        let Some(current_route_target) = planned_route.current() else {
            steering.desired_velocity = Vec3::ZERO;
            continue;
        };
        let navigation_agent_position = global_transform.translation();
        let mut route_target_offset = current_route_target - navigation_agent_position;
        if locomotion_mode.copied().unwrap_or_default() != LocomotionMode::Flight {
            route_target_offset.y = 0.0;
        }
        let mut desired_velocity =
            route_target_offset.normalize_or_zero() * navigation_agent.max_speed_mps;
        desired_velocity +=
            contact_steering.direction * navigation_agent.max_speed_mps * contact_steering.factor;
        let mut visited_neighbor_count = 0;
        // Reach far enough to see a large neighbour, such as an elephant,
        // before walking into it.
        let reach = (((navigation_agent.radius_m + LARGEST_NEIGHBOR_RADIUS_M)
            / spatial_grid.cell_size_m)
            .ceil() as u32)
            .clamp(1, 4);
        if let Some(spatial_cell_coordinates) = spatial_grid.cell_xy(spatial_cell.0) {
            'neighboring_cells: for neighboring_cell_z in
                spatial_cell_coordinates.y.saturating_sub(reach)
                    ..=(spatial_cell_coordinates.y + reach).min(spatial_grid.height - 1)
            {
                for neighboring_cell_x in spatial_cell_coordinates.x.saturating_sub(reach)
                    ..=(spatial_cell_coordinates.x + reach).min(spatial_grid.width - 1)
                {
                    let neighboring_cell_index =
                        neighboring_cell_z * spatial_grid.width + neighboring_cell_x;
                    for neighboring_agent_entity in spatial_grid.range(neighboring_cell_index) {
                        if *neighboring_agent_entity == navigation_agent_entity {
                            continue;
                        }
                        let Ok((neighboring_global_transform, neighboring_navigation_agent)) =
                            neighboring_navigation_agents.get(*neighboring_agent_entity)
                        else {
                            continue;
                        };
                        let neighboring_agent_offset =
                            navigation_agent_position - neighboring_global_transform.translation();
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
                                * (1.0 - neighboring_agent_distance / required_separation_distance);
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
    }
}

/// How far an overlapping idle guest moves aside in one update.
const IDLE_SEPARATION_STEP_M: f32 = 0.02;

/// Guests standing still, such as several watching an animal from one spot,
/// step apart until they no longer stand in each other. A guest holding an
/// interaction slot, such as a stand's counter or a bench, stays put.
#[allow(clippy::type_complexity)]
pub(super) fn separate_overlapping_idle_guests(
    spatial_grid: Res<SpatialGrid>,
    mut guests: Query<
        (
            Entity,
            &NavAgent,
            &GlobalTransform,
            &SpatialCell,
            &mut Transform,
            Option<&openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState>,
        ),
        (
            With<openzt2_zoo_state::guests::guest_simulation_types::Guest>,
            Without<Destination>,
            Without<openzt2_zoo_state::locomotion::locomotion_types::Docking>,
            Without<DirectLocomotion>,
        ),
    >,
    neighbors: Query<(&GlobalTransform, &NavAgent)>,
) {
    let mut pushes = Vec::new();
    for (guest, agent, global, cell, _, task) in &guests {
        if task.is_some_and(|task| task.interaction_slot.is_some()) {
            continue;
        }
        let Some(coordinates) = spatial_grid.cell_xy(cell.0) else {
            continue;
        };
        let position = global.translation();
        let mut push = Vec3::ZERO;
        for z in coordinates.y.saturating_sub(1)..=(coordinates.y + 1).min(spatial_grid.height - 1)
        {
            for x in
                coordinates.x.saturating_sub(1)..=(coordinates.x + 1).min(spatial_grid.width - 1)
            {
                for &neighbor in spatial_grid.range(z * spatial_grid.width + x) {
                    if neighbor == guest {
                        continue;
                    }
                    let Ok((neighbor_global, neighbor_agent)) = neighbors.get(neighbor) else {
                        continue;
                    };
                    let mut offset = position - neighbor_global.translation();
                    offset.y = 0.0;
                    let required = agent.radius_m + neighbor_agent.radius_m;
                    let distance = offset.length();
                    if distance >= required {
                        continue;
                    }
                    // Guests on the very same spot part along a direction
                    // drawn from their entities, the same every update.
                    let direction = if distance > 0.0001 {
                        offset / distance
                    } else {
                        let angle = (guest.to_bits() ^ neighbor.to_bits()) as f32;
                        let sign = if guest.to_bits() < neighbor.to_bits() {
                            1.0
                        } else {
                            -1.0
                        };
                        Vec3::new(angle.cos(), 0.0, angle.sin()) * sign
                    };
                    push += direction * (required - distance);
                }
            }
        }
        if push != Vec3::ZERO {
            pushes.push((guest, push.clamp_length_max(IDLE_SEPARATION_STEP_M)));
        }
    }
    for (guest, push) in pushes {
        if let Ok((_, _, _, _, mut transform, _)) = guests.get_mut(guest) {
            transform.translation += push;
        }
    }
}
