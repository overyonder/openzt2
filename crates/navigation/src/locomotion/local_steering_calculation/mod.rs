use bevy::prelude::*;

use openzt2_zoo_state::locomotion::locomotion_types::ContactSteering;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::DirectLocomotion;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavFlags;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::locomotion_types::SpatialCell;
use openzt2_zoo_state::locomotion::locomotion_types::SpatialGrid;
use openzt2_zoo_state::locomotion::locomotion_types::Steering;
use openzt2_zoo_state::locomotion::locomotion_types::MAX_STEERING_NEIGHBORS;

/// The largest agent radius separation looks out for.
const LARGEST_NEIGHBOR_RADIUS_M: f32 = 3.0;
/// A person's authored collision radius, about 10 cm, is far narrower than
/// the body drawn around it; keeping apart by it leaves people inside each
/// other. Spacing treats everyone as at least this wide.
const MINIMUM_SPACING_RADIUS_M: f32 = 0.3;

fn spacing_radius(agent: &NavAgent) -> f32 {
    agent.radius_m.max(MINIMUM_SPACING_RADIUS_M)
}

/// Within this many spacing radii of a destination someone stands on, an
/// approaching agent counts as arrived.
const OCCUPIED_ARRIVAL_SPACINGS: f32 = 4.0;

pub(super) fn calculate_route_following_and_separation_steering(
    spatial_grid: Res<SpatialGrid>,
    mut route_following_navigation_agents: Query<
        (
            Entity,
            &NavAgent,
            &GlobalTransform,
            &SpatialCell,
            &mut Route,
            &Destination,
            Option<&LocomotionMode>,
            &mut ContactSteering,
            &mut Steering,
            Has<openzt2_zoo_state::locomotion::locomotion_types::Docking>,
        ),
        Without<DirectLocomotion>,
    >,
    neighboring_navigation_agents: Query<(&GlobalTransform, &NavAgent, Has<Destination>)>,
    time: Res<Time<Fixed>>,
) {
    for (
        navigation_agent_entity,
        navigation_agent,
        global_transform,
        spatial_cell,
        mut planned_route,
        destination,
        locomotion_mode,
        mut contact_steering,
        mut steering,
        docking,
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
        let route_direction = route_target_offset.normalize_or_zero();
        let mut desired_velocity = route_direction * navigation_agent.max_speed_mps;
        desired_velocity +=
            contact_steering.direction * navigation_agent.max_speed_mps * contact_steering.factor;
        // Only people stop short of an occupied spot, such as a viewpoint;
        // an animal walking to a dish or a keeper's site still arrives. A
        // dock, such as a place in a stand's queue, completes only on its
        // spot, which whoever holds it is about to leave, so it waits there.
        let is_person =
            navigation_agent.capabilities.0 & (NavFlags::GUEST.0 | NavFlags::STAFF.0) != 0;
        let final_approach = is_person
            && !docking
            && planned_route.cursor + 1 == planned_route.points.len()
            && navigation_agent_position
                .xz()
                .distance(destination.world.xz())
                < spacing_radius(navigation_agent) * OCCUPIED_ARRIVAL_SPACINGS;
        let mut destination_occupied = false;
        let mut person_contact = false;
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
                        let Ok((
                            neighboring_global_transform,
                            neighboring_navigation_agent,
                            neighbor_moving,
                        )) = neighboring_navigation_agents.get(*neighboring_agent_entity)
                        else {
                            continue;
                        };
                        // Someone already standing on the spot, such as a
                        // guest at the one viewpoint of an exhibit, leaves
                        // the next one to stop beside them, not push in.
                        destination_occupied |= final_approach
                            && !neighbor_moving
                            && neighboring_global_transform
                                .translation()
                                .xz()
                                .distance(destination.world.xz())
                                < spacing_radius(navigation_agent)
                                    + spacing_radius(neighboring_navigation_agent);
                        let neighboring_agent_offset =
                            navigation_agent_position - neighboring_global_transform.translation();
                        let neighboring_agent_distance_squared =
                            neighboring_agent_offset.length_squared();
                        let required_separation_distance = spacing_radius(navigation_agent)
                            + spacing_radius(neighboring_navigation_agent);
                        // Count only neighbours touching this one, so a crowd
                        // further off does not use up the budget first.
                        if neighboring_agent_distance_squared
                            >= required_separation_distance * required_separation_distance
                        {
                            continue;
                        }
                        // Like the original's collision handler, a walker is
                        // pushed off whoever it touches, standing or walking,
                        // but never backwards: a push against its heading
                        // turns into a step to the side it leans towards, so
                        // two people meeting head-on slip past each other.
                        let neighboring_agent_distance = neighboring_agent_distance_squared.sqrt();
                        let mut push = if neighboring_agent_distance > 0.0001 {
                            neighboring_agent_offset / neighboring_agent_distance
                        } else {
                            -route_direction
                        };
                        push.y = 0.0;
                        if push.dot(route_direction) < 0.0 {
                            let aside = Vec3::new(route_direction.z, 0.0, -route_direction.x);
                            push = if push.dot(aside) < 0.0 { -aside } else { aside };
                        }
                        person_contact = true;
                        desired_velocity += push
                            * navigation_agent.max_speed_mps
                            * contact_steering.factor
                            * (1.0 - neighboring_agent_distance / required_separation_distance);
                        visited_neighbor_count += 1;
                        if visited_neighbor_count == MAX_STEERING_NEIGHBORS {
                            break 'neighboring_cells;
                        }
                    }
                }
            }
        }
        // Staying pressed against someone weakens the push, as it does for
        // any other contact, so a jam eventually lets the walkers through.
        if person_contact {
            contact_steering.factor = (contact_steering.factor
                - ContactSteering::DECAY_PER_SECOND * time.delta_secs())
            .max(ContactSteering::MINIMUM_FACTOR);
            contact_steering.recovery_delay_s = ContactSteering::RECOVERY_DELAY_SECONDS;
        }
        if destination_occupied {
            planned_route.cursor = planned_route.points.len();
            steering.desired_velocity = Vec3::ZERO;
            continue;
        }
        steering.desired_velocity =
            desired_velocity.clamp_length_max(navigation_agent.max_speed_mps);
    }
}

/// How far an overlapping idle guest moves aside in one update.
const IDLE_SEPARATION_STEP_M: f32 = 0.02;

/// People standing still, such as several guests watching an animal from one
/// spot or two keepers idling at the same place, step apart until they no
/// longer stand in each other. Someone holding an interaction slot, such as a
/// stand's counter or a bench, stays put.
#[allow(clippy::type_complexity)]
pub(super) fn separate_overlapping_idle_people(
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
            Or<(
                With<openzt2_zoo_state::guests::guest_simulation_types::Guest>,
                With<openzt2_zoo_state::staff::staff_employment_types::Staff>,
            )>,
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
                    let required = spacing_radius(agent) + spacing_radius(neighbor_agent);
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

#[cfg(test)]
mod avoidance_tests {
    use super::*;

    #[test]
    fn people_meeting_head_on_step_to_opposite_sides() {
        let mut world = World::new();
        let agent = NavAgent {
            radius_m: 0.3,
            max_speed_mps: 1.0,
            acceleration_mps2: 1.0,
            capabilities: openzt2_zoo_state::locomotion::locomotion_types::NavFlags::GUEST,
        };
        let walker = |world: &mut World, from: f32, to: f32| {
            let mut route = Route::with_capacity(1);
            route.points.push(Vec3::new(to, 0.0, 0.0));
            world
                .spawn((
                    agent,
                    GlobalTransform::from_translation(Vec3::new(from, 0.0, 0.0)),
                    SpatialCell(0),
                    route,
                    Destination {
                        request_id: 1,
                        world: Vec3::new(to, 0.0, 0.0),
                        arrival_radius_m: 0.1,
                    },
                    ContactSteering::default(),
                    Steering::default(),
                ))
                .id()
        };
        // Touching, face to face.
        let east = walker(&mut world, -0.2, 10.0);
        let west = walker(&mut world, 0.2, -10.0);
        let mut time = Time::<Fixed>::default();
        time.advance_by(std::time::Duration::from_millis(100));
        world.insert_resource(time);
        world.insert_resource(SpatialGrid {
            width: 1,
            height: 1,
            cell_offsets: Box::new([0, 2]),
            write_cursors: Box::new([2]),
            entities: vec![east, west],
            ..SpatialGrid::default()
        });
        let mut system = IntoSystem::into_system(calculate_route_following_and_separation_steering);
        system.initialize(&mut world);
        system.run((), &mut world).unwrap();
        let steer = |entity| world.get::<Steering>(entity).unwrap().desired_velocity;
        // The same side of each heading: -Z going east, +Z going west.
        assert!(steer(east).z < 0.0, "{:?}", steer(east));
        assert!(steer(west).z > 0.0, "{:?}", steer(west));
        // Neither is pushed back the way it came.
        assert!(steer(east).x > 0.0 && steer(west).x < 0.0);
        // Staying pressed together weakens the push.
        assert!(world.get::<ContactSteering>(east).unwrap().factor < 1.0);
    }

    #[test]
    fn standing_guests_part_wider_than_their_collision_radius() {
        let mut world = World::new();
        let agent = NavAgent {
            radius_m: 0.1,
            max_speed_mps: 1.0,
            acceleration_mps2: 1.0,
            capabilities: openzt2_zoo_state::locomotion::locomotion_types::NavFlags::GUEST,
        };
        let guest = |world: &mut World, x: f32| {
            world
                .spawn((
                    openzt2_zoo_state::guests::guest_simulation_types::Guest,
                    agent,
                    GlobalTransform::from_translation(Vec3::X * x),
                    Transform::from_translation(Vec3::X * x),
                    SpatialCell(0),
                ))
                .id()
        };
        let left = guest(&mut world, 0.0);
        let right = guest(&mut world, 0.25);
        world.insert_resource(SpatialGrid {
            width: 1,
            height: 1,
            cell_offsets: Box::new([0, 2]),
            write_cursors: Box::new([2]),
            entities: vec![left, right],
            ..SpatialGrid::default()
        });
        let mut system = IntoSystem::into_system(separate_overlapping_idle_people);
        system.initialize(&mut world);
        system.run((), &mut world).unwrap();
        assert!(world.get::<Transform>(left).unwrap().translation.x < 0.0);
        assert!(world.get::<Transform>(right).unwrap().translation.x > 0.25);
    }

    /// Whether a walker with `capabilities`, docking or not, stops beside
    /// someone standing on its destination, one step from it.
    fn stops_beside_occupied_destination(
        capabilities: openzt2_zoo_state::locomotion::locomotion_types::NavFlags,
        docking: bool,
    ) -> bool {
        let mut world = World::new();
        let agent = NavAgent {
            radius_m: 0.1,
            max_speed_mps: 1.0,
            acceleration_mps2: 1.0,
            capabilities,
        };
        let spot = Vec3::new(2.0, 0.0, 0.0);
        let mut route = Route::with_capacity(1);
        route.points.push(spot);
        let walker = world
            .spawn((
                agent,
                GlobalTransform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
                SpatialCell(0),
                route,
                Destination {
                    request_id: 1,
                    world: spot,
                    arrival_radius_m: 0.1,
                },
                ContactSteering::default(),
                Steering::default(),
            ))
            .id();
        if docking {
            let target = world.spawn_empty().id();
            world.entity_mut(walker).insert(
                openzt2_zoo_state::locomotion::locomotion_types::Docking {
                    request_id: 1,
                    target,
                    point: spot,
                    forward: Vec3::Z,
                    radius_m: 0.1,
                },
            );
        }
        let stander = world
            .spawn((
                agent,
                GlobalTransform::from_translation(spot),
                SpatialCell(0),
            ))
            .id();
        world.insert_resource(SpatialGrid {
            width: 1,
            height: 1,
            cell_offsets: Box::new([0, 2]),
            write_cursors: Box::new([2]),
            entities: vec![walker, stander],
            ..SpatialGrid::default()
        });
        world.insert_resource(Time::<Fixed>::default());
        let mut system = IntoSystem::into_system(calculate_route_following_and_separation_steering);
        system.initialize(&mut world);
        system.run((), &mut world).unwrap();
        let route = world.get::<Route>(walker).unwrap();
        route.cursor == route.points.len()
    }

    #[test]
    fn a_walker_stops_beside_someone_standing_on_its_destination() {
        use openzt2_zoo_state::locomotion::locomotion_types::NavFlags;
        assert!(stops_beside_occupied_destination(NavFlags::GUEST, false));
        // An animal still walks up to its dish, as to a keeper's site.
        assert!(!stops_beside_occupied_destination(NavFlags::ANIMAL, false));
        // A dock completes only on its spot, so a guest waits for a place in
        // a queue rather than stopping beside it for good.
        assert!(!stops_beside_occupied_destination(NavFlags::GUEST, true));
    }
}
