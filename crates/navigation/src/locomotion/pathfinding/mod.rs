use bevy::prelude::*;

use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavFlags;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure;
use openzt2_zoo_state::locomotion::locomotion_types::OpenNode;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::locomotion_types::RouteScratch;
use openzt2_zoo_state::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph;
use openzt2_zoo_state::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraphEdgeFlags as NavigationGraphEdgeFlags;
use openzt2_zoo_state::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraphNode;
use openzt2_zoo_state::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraphTraversalKind;

pub(crate) fn plan_route(
    navigation: &TerrainDerivedNavigationGraph,
    edge_is_blocked: impl Fn(u32, u32) -> bool,
    world_segment_is_blocked: impl Fn(Vec3, Vec3) -> bool,
    agent: &NavAgent,
    mode: LocomotionMode,
    maximum_ground_slope_radians: Option<f32>,
    start_world: Vec3,
    destination: Vec3,
    route: &mut Route,
    scratch: &mut RouteScratch,
) -> Result<(), NavigationFailure> {
    if !start_world.is_finite() || !destination.is_finite() {
        return Err(NavigationFailure::OutsideWorld);
    }
    let minimum_normal_y = minimum_ground_normal_y(agent, mode, maximum_ground_slope_radians)?;
    let start = nearest_eligible(navigation, agent, mode, start_world, minimum_normal_y)
        .ok_or(NavigationFailure::NoStartNode)?;
    let goal = nearest_eligible(navigation, agent, mode, destination, minimum_normal_y)
        .ok_or(NavigationFailure::OutsideWorld)?;
    // Docking may deliberately snap a non-animal endpoint to a nearby node.
    // Its final exact segment still cannot take a ground actor through water.
    if matches!(mode, LocomotionMode::Ground | LocomotionMode::Vehicle)
        && !agent.capabilities.contains(NavFlags::AMPHIBIOUS)
        && navigation.path_surface_at_node(goal).is_none()
        && !ground_endpoint_segment_is_dry(
            navigation,
            navigation_position(navigation, goal),
            destination,
        )
    {
        route.clear();
        return Err(NavigationFailure::NoRoute);
    }
    if scratch.costs.len() != navigation.navigation_node_count()
        || scratch.previous.len() != navigation.navigation_node_count()
    {
        return Err(NavigationFailure::NoRoute);
    }

    scratch.begin();
    visit(scratch, start, 0, u32::MAX);
    if !heap_push_or_decrease(
        &mut scratch.open,
        &mut scratch.heap_positions,
        OpenNode {
            node: start,
            cost: 0,
            estimate: heuristic(navigation, start, goal),
        },
    ) {
        return Err(NavigationFailure::NoRoute);
    }

    let mut reached = false;
    while let Some(current) = heap_pop(&mut scratch.open, &mut scratch.heap_positions) {
        if scratch.marks[current.node as usize] != scratch.generation
            || scratch.costs[current.node as usize] != current.cost
        {
            continue;
        }
        if current.node == goal {
            reached = true;
            break;
        }
        scratch.closed[current.node as usize] = true;
        for edge in navigation.iterate_outgoing_navigation_graph_edges(current.node) {
            let next = edge.destination_node;
            if scratch.closed[next as usize]
                || edge_is_blocked(current.node, next)
                || edge.edge_flags.0 & NavigationGraphEdgeFlags::DISABLED_BY_DEFAULT.0 != 0
                || !(agent.capabilities.contains(NavFlags::AMPHIBIOUS)
                    || traversal_allowed(edge.traversal_kind, mode))
            {
                continue;
            }
            let Some(next_node) = navigation.find_navigation_node(next) else {
                continue;
            };
            if next_node.eligibility_flags.0 & agent.capabilities.0 == 0
                || f32::from(next_node.clearance_centimetres) < agent.radius_m * 100.0
                || f32::from(edge.width_centimetres) < agent.radius_m * 200.0
                || !(agent.capabilities.contains(NavFlags::AMPHIBIOUS)
                    && next_node.eligibility_flags.contains(NavFlags::WATER))
                    && !cell_meets_ground_slope_limit(
                        next_node.minimum_triangle_normal_y,
                        minimum_normal_y,
                    )
            {
                continue;
            }
            let Some(cost) = current
                .cost
                .checked_add(u64::from(edge.traversal_cost_millimetres))
            else {
                continue;
            };
            let slot = next as usize;
            let old = if scratch.marks[slot] == scratch.generation {
                scratch.costs[slot]
            } else {
                u64::MAX
            };
            let old_previous = scratch.previous[slot];
            if cost < old || (cost == old && current.node < old_previous) {
                visit(scratch, next, cost, current.node);
                if !heap_push_or_decrease(
                    &mut scratch.open,
                    &mut scratch.heap_positions,
                    OpenNode {
                        node: next,
                        cost,
                        estimate: cost.saturating_add(heuristic(navigation, next, goal)),
                    },
                ) {
                    route.clear();
                    return Err(NavigationFailure::NoRoute);
                }
            }
        }
    }
    if !reached {
        route.clear();
        return Err(NavigationFailure::NoRoute);
    }

    let mut node = goal;
    loop {
        if scratch.reverse_path.len() == scratch.reverse_path.capacity() {
            route.clear();
            return Err(NavigationFailure::NoRoute);
        }
        scratch.reverse_path.push(node);
        if node == start {
            break;
        }
        node = scratch.previous[node as usize];
        if node == u32::MAX {
            route.clear();
            return Err(NavigationFailure::NoRoute);
        }
    }

    route.clear();
    if route.points.capacity() < scratch.reverse_path.len().saturating_add(1) {
        return Err(NavigationFailure::NoRoute);
    }
    // Retain the searched start cell for every actor. Snapped endpoints must
    // not create an unchecked shortcut through a fence or closed gate.
    for node in scratch.reverse_path.iter().rev() {
        push_if_turn(navigation_position(navigation, *node), &mut route.points);
    }
    push_if_turn(destination, &mut route.points);
    let mut previous = start_world;
    let crosses_boundary = route.points.iter().copied().any(|point| {
        let blocked = world_segment_is_blocked(previous, point);
        previous = point;
        blocked
    });
    if crosses_boundary {
        route.clear();
        return Err(NavigationFailure::NoRoute);
    }
    Ok(())
}

/// Follows connected navigation surfaces from the start and checks the
/// collision circle on each sample. The final surface must own the target.
pub(crate) fn straight_line_sweep_admits_agent(
    navigation: &TerrainDerivedNavigationGraph,
    agent: &NavAgent,
    mode: LocomotionMode,
    maximum_ground_slope_radians: Option<f32>,
    from: Vec3,
    to: Vec3,
    target_mode: Option<LocomotionMode>,
) -> bool {
    let step_m = agent.radius_m;
    if step_m <= 0.0 {
        return true;
    }
    if !step_m.is_finite() {
        return false;
    }
    let Ok(minimum_normal_y) = minimum_ground_normal_y(agent, mode, maximum_ground_slope_radians)
    else {
        return false;
    };
    if !from.is_finite() || !to.is_finite() {
        return false;
    }
    let containing_surface = |position| {
        if mode == LocomotionMode::Swim {
            navigation
                .terrain_cell_at_world_position(position)
                .and_then(|cell| navigation.calculate_navigation_node_for_terrain_cell(cell))
        } else {
            navigation.navigation_surface_at_world_position(position)
        }
    };
    let Some(mut surface) = containing_surface(from) else {
        return false;
    };
    let target_surface = if target_mode == Some(LocomotionMode::Swim) {
        navigation
            .terrain_cell_at_world_position(to)
            .and_then(|cell| navigation.calculate_navigation_node_for_terrain_cell(cell))
    } else {
        navigation.navigation_surface_at_world_position(to)
    };
    let Some(target_surface) = target_surface else {
        return false;
    };
    let amphibious = agent.capabilities.contains(NavFlags::AMPHIBIOUS);
    let admits = |index| {
        navigation.find_navigation_node(index).is_some_and(|node| {
            let water = node.eligibility_flags.contains(NavFlags::WATER);
            node_admits_agent(node, agent, minimum_normal_y)
                && (amphibious
                    || match mode {
                        LocomotionMode::Ground | LocomotionMode::Vehicle => !water,
                        LocomotionMode::Swim => water,
                        LocomotionMode::Flight => true,
                    })
        })
    };
    let offset = (to - from).xz();
    let length_m = offset.length();
    let direction = offset.normalize_or_zero();
    // A radius larger than a cell must not skip an intervening surface edge.
    let step_m = step_m.min(terrain_cell_size_m(navigation) * 0.5);
    let mut travelled_m = 0.0;
    loop {
        travelled_m = (travelled_m + step_m).min(length_m);
        let mut sample = from + Vec3::new(direction.x, 0.0, direction.y) * travelled_m;
        sample.y = from.y
            + (to.y - from.y)
                * if length_m > 0.0 {
                    travelled_m / length_m
                } else {
                    1.0
                };
        let Some(next) = navigation.continue_navigation_surface(surface, sample) else {
            return false;
        };
        surface = next;
        if !admits(surface) {
            return false;
        }
        if navigation.path_surface_at_node(surface).is_some() {
            for direction in [
                Vec2::X,
                Vec2::Y,
                Vec2::NEG_X,
                Vec2::NEG_Y,
                Vec2::ONE.normalize(),
                Vec2::new(1.0, -1.0).normalize(),
                Vec2::new(-1.0, 1.0).normalize(),
                Vec2::NEG_ONE.normalize(),
            ] {
                let occupied = sample + Vec3::new(direction.x, 0.0, direction.y) * agent.radius_m;
                let Some(node) = navigation.continue_navigation_surface(surface, occupied) else {
                    return false;
                };
                if !admits(node) {
                    return false;
                }
            }
        }
        // Sample the occupied circle as well as its centre. Terrain cell
        // squares intersecting it all contribute to the same clearance test.
        let radius = agent.radius_m;
        let size = terrain_cell_size_m(navigation);
        let minimum = ((Vec2::new(sample.x - radius, -sample.z - radius)) / size)
            .floor()
            .as_ivec2();
        let maximum = ((Vec2::new(sample.x + radius, -sample.z + radius)) / size)
            .floor()
            .as_ivec2();
        for z in minimum.y..=maximum.y {
            for x in minimum.x..=maximum.x {
                let low = Vec2::new(x as f32, z as f32) * size;
                let centre = Vec2::new(sample.x, -sample.z);
                let closest = centre.clamp(low, low + Vec2::splat(size));
                if closest.distance_squared(centre) > radius * radius {
                    continue;
                }
                let occupied = Vec3::new(closest.x, sample.y, -closest.y);
                let Some(node) = navigation.continue_navigation_surface(surface, occupied) else {
                    return false;
                };
                if !admits(node) {
                    return false;
                }
            }
        }
        if travelled_m >= length_m {
            return surface == target_surface
                || navigation.node_supports_world_position(surface, to)
                    && navigation
                        .iterate_outgoing_navigation_graph_edges(surface)
                        .any(|edge| {
                            edge.destination_node == target_surface
                                && navigation.node_supports_world_position(target_surface, to)
                        });
        }
    }
}

fn ground_endpoint_segment_is_dry(
    navigation: &TerrainDerivedNavigationGraph,
    from: Vec3,
    to: Vec3,
) -> bool {
    every_crossed_terrain_cell(navigation, from, to, |cell| {
        terrain_cell_node(navigation, cell)
            .is_some_and(|node| !node.eligibility_flags.contains(NavFlags::WATER))
    })
}

fn terrain_cell_size_m(navigation: &TerrainDerivedNavigationGraph) -> f32 {
    f32::from(navigation.terrain_cell_size_centimetres) * 0.01
}

fn terrain_cell_node(
    navigation: &TerrainDerivedNavigationGraph,
    cell: IVec2,
) -> Option<&TerrainDerivedNavigationGraphNode> {
    if cell.x < 0 || cell.y < 0 {
        return None;
    }
    navigation
        .calculate_navigation_node_for_terrain_cell(cell.as_uvec2().to_array())
        .and_then(|index| navigation.find_navigation_node(index))
}

fn every_crossed_terrain_cell(
    navigation: &TerrainDerivedNavigationGraph,
    from: Vec3,
    to: Vec3,
    accepts: impl Fn(IVec2) -> bool,
) -> bool {
    let size = terrain_cell_size_m(navigation);
    if size <= 0.0
        || !from.is_finite()
        || !to.is_finite()
        || from.x < 0.0
        || to.x < 0.0
        || from.z > 0.0
        || to.z > 0.0
    {
        return false;
    }
    let from = Vec2::new(from.x, -from.z) / size;
    let to = Vec2::new(to.x, -to.z) / size;
    let mut cell = from.floor().as_ivec2();
    let end = to.floor().as_ivec2();
    let delta = to - from;
    let step = IVec2::new(delta.x.signum() as i32, delta.y.signum() as i32);
    let next_boundary = |position: f32, coordinate: i32, direction: f32| {
        if direction == 0.0 {
            f32::INFINITY
        } else {
            ((coordinate + i32::from(direction > 0.0)) as f32 - position) / direction
        }
    };
    let mut crossing = Vec2::new(
        next_boundary(from.x, cell.x, delta.x),
        next_boundary(from.y, cell.y, delta.y),
    );
    let interval = Vec2::new(delta.x.abs().recip(), delta.y.abs().recip());
    // Traverse crossed grid cells, including both neighbours at exact corners.
    loop {
        if !accepts(cell) {
            return false;
        }
        if cell == end {
            return true;
        }
        if crossing.x == crossing.y {
            if !accepts(cell + IVec2::new(step.x, 0)) || !accepts(cell + IVec2::new(0, step.y)) {
                return false;
            }
            cell += step;
            crossing += interval;
        } else if crossing.x < crossing.y {
            cell.x += step.x;
            crossing.x += interval.x;
        } else {
            cell.y += step.y;
            crossing.y += interval.y;
        }
    }
}

fn minimum_ground_normal_y(
    agent: &NavAgent,
    mode: LocomotionMode,
    maximum_ground_slope_radians: Option<f32>,
) -> Result<Option<f32>, NavigationFailure> {
    if mode != LocomotionMode::Ground && !agent.capabilities.contains(NavFlags::AMPHIBIOUS) {
        return Ok(None);
    }
    maximum_ground_slope_radians
        .map(|slope| {
            if !slope.is_finite() || slope < 0.0 {
                return Err(NavigationFailure::NoRoute);
            }
            Ok(slope.min(std::f32::consts::FRAC_PI_2).cos().max(0.0))
        })
        .transpose()
}

fn node_admits_agent(
    node: &TerrainDerivedNavigationGraphNode,
    agent: &NavAgent,
    minimum_normal_y: Option<f32>,
) -> bool {
    node.eligibility_flags.0 & agent.capabilities.0 != 0
        && node.eligibility_flags.0 & NavFlags::DISABLED_BY_DEFAULT.0 == 0
        && f32::from(node.clearance_centimetres) >= agent.radius_m * 100.0
        && ((agent.capabilities.contains(NavFlags::AMPHIBIOUS)
            && node.eligibility_flags.contains(NavFlags::WATER))
            || cell_meets_ground_slope_limit(node.minimum_triangle_normal_y, minimum_normal_y))
}

fn nearest_eligible(
    navigation: &TerrainDerivedNavigationGraph,
    agent: &NavAgent,
    mode: LocomotionMode,
    world: Vec3,
    minimum_normal_y: Option<f32>,
) -> Option<u32> {
    let position = metres_to_cm(world)?;
    if let Some(surface) = navigation.navigation_surface_at_world_position(world) {
        if mode != LocomotionMode::Swim && navigation.path_surface_at_node(surface).is_some() {
            return navigation
                .find_navigation_node(surface)
                .filter(|node| node_admits_agent(node, agent, minimum_normal_y))
                .map(|_| surface);
        }
    }
    if minimum_normal_y.is_some() {
        // Snapping an animal to another cell can bypass a forbidden endpoint
        // and make the final direct segment cross terrain the search rejected.
        if world.x < 0.0 || world.z > 0.0 {
            return None;
        }
        let cell_size_m = f32::from(navigation.terrain_cell_size_centimetres) * 0.01;
        let cell = [
            (world.x / cell_size_m).floor() as u32,
            (-world.z / cell_size_m).floor() as u32,
        ];
        let index = navigation.calculate_navigation_node_for_terrain_cell(cell)?;
        let node = navigation.find_navigation_node(index)?;
        return node_admits_agent(node, agent, minimum_normal_y).then_some(index);
    }
    navigation
        .find_candidate_node_indices_in_containing_terrain_tile(position)
        .filter(|index| {
            if mode == LocomotionMode::Swim && navigation.path_surface_at_node(*index).is_some() {
                return false;
            }
            navigation.find_navigation_node(*index).is_some_and(|node| {
                node.eligibility_flags.0 & agent.capabilities.0 != 0
                    && node.eligibility_flags.0 & NavFlags::DISABLED_BY_DEFAULT.0 == 0
                    && f32::from(node.clearance_centimetres) >= agent.radius_m * 100.0
            })
        })
        .min_by_key(|index| {
            (
                navigation
                    .calculate_navigation_node_world_position_centimetres(*index)
                    .map_or(u128::MAX, |p| distance_squared_cm(p, position)),
                *index,
            )
        })
}

fn cell_meets_ground_slope_limit(normal_y: f32, minimum_normal_y: Option<f32>) -> bool {
    minimum_normal_y.is_none_or(|minimum| normal_y.is_finite() && normal_y >= minimum)
}

fn traversal_allowed(
    traversal: TerrainDerivedNavigationGraphTraversalKind,
    mode: LocomotionMode,
) -> bool {
    match traversal {
        TerrainDerivedNavigationGraphTraversalKind::Shore => false,
        TerrainDerivedNavigationGraphTraversalKind::Swim => mode == LocomotionMode::Swim,
        TerrainDerivedNavigationGraphTraversalKind::Ground
        | TerrainDerivedNavigationGraphTraversalKind::Path => mode != LocomotionMode::Swim,
    }
}

fn visit(scratch: &mut RouteScratch, node: u32, cost: u64, previous: u32) {
    let slot = node as usize;
    scratch.marks[slot] = scratch.generation;
    scratch.costs[slot] = cost;
    scratch.previous[slot] = previous;
}

fn heuristic(navigation: &TerrainDerivedNavigationGraph, a: u32, b: u32) -> u64 {
    let Some((a, b)) = navigation
        .calculate_navigation_node_world_position_centimetres(a)
        .zip(navigation.calculate_navigation_node_world_position_centimetres(b))
    else {
        return u64::MAX;
    };
    let squared = a.into_iter().zip(b).fold(0_u128, |sum, (a, b)| {
        let delta = i128::from(a) - i128::from(b);
        sum + (delta * delta) as u128
    });
    u64::try_from(squared.isqrt().saturating_mul(10)).unwrap_or(u64::MAX)
}

fn metres_to_cm(point: Vec3) -> Option<[i32; 3]> {
    if !point.is_finite() {
        return None;
    }
    Some([
        (point.x * 100.0).round() as i32,
        (point.y * 100.0).round() as i32,
        (point.z * 100.0).round() as i32,
    ])
}

fn navigation_position(navigation: &TerrainDerivedNavigationGraph, node: u32) -> Vec3 {
    navigation
        .calculate_navigation_node_world_position_centimetres(node)
        .map_or(Vec3::ZERO, |p| {
            Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32) * 0.01
        })
}

fn distance_squared_cm(a: [i32; 3], b: [i32; 3]) -> u128 {
    a.into_iter().zip(b).fold(0_u128, |sum, (a, b)| {
        let delta = i128::from(a) - i128::from(b);
        sum + (delta * delta) as u128
    })
}

fn push_if_turn(point: Vec3, points: &mut Vec<Vec3>) {
    if points.len() >= 2 {
        let a = points[points.len() - 2];
        let b = points[points.len() - 1];
        let ab = (b - a).normalize_or_zero();
        let bc = (point - b).normalize_or_zero();
        if ab.dot(bc) > 0.9999 {
            let last = points.len() - 1;
            points[last] = point;
            return;
        }
    }
    points.push(point);
}

fn heap_push_or_decrease(heap: &mut Vec<OpenNode>, positions: &mut [u32], value: OpenNode) -> bool {
    let slot = value.node as usize;
    let existing = positions[slot];
    let mut index = if existing != u32::MAX {
        let index = existing as usize;
        if open_key(heap[index]) <= open_key(value) {
            return true;
        }
        heap[index] = value;
        index
    } else {
        if heap.len() == heap.capacity() {
            return false;
        }
        heap.push(value);
        let index = heap.len() - 1;
        positions[slot] = index as u32;
        index
    };
    while index > 0 {
        let parent = (index - 1) / 2;
        if open_key(heap[parent]) <= open_key(heap[index]) {
            break;
        }
        heap.swap(parent, index);
        positions[heap[parent].node as usize] = parent as u32;
        positions[heap[index].node as usize] = index as u32;
        index = parent;
    }
    true
}

fn heap_pop(heap: &mut Vec<OpenNode>, positions: &mut [u32]) -> Option<OpenNode> {
    let last = heap.pop()?;
    if heap.is_empty() {
        positions[last.node as usize] = u32::MAX;
        return Some(last);
    }
    let result = std::mem::replace(&mut heap[0], last);
    positions[result.node as usize] = u32::MAX;
    positions[heap[0].node as usize] = 0;
    let mut index = 0;
    loop {
        let left = index * 2 + 1;
        if left >= heap.len() {
            break;
        }
        let right = left + 1;
        let child = if right < heap.len() && open_key(heap[right]) < open_key(heap[left]) {
            right
        } else {
            left
        };
        if open_key(heap[index]) <= open_key(heap[child]) {
            break;
        }
        heap.swap(index, child);
        positions[heap[index].node as usize] = index as u32;
        positions[heap[child].node as usize] = child as u32;
        index = child;
    }
    Some(result)
}

fn open_key(node: OpenNode) -> (u64, u64, u32) {
    (node.estimate, node.cost, node.node)
}

#[cfg(test)]
mod shoreline_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heap_order_is_stable() {
        let mut heap = Vec::with_capacity(4);
        let mut positions = vec![u32::MAX; 4];
        assert!(heap_push_or_decrease(
            &mut heap,
            &mut positions,
            OpenNode {
                node: 3,
                cost: 2,
                estimate: 5,
            },
        ));
        assert!(heap_push_or_decrease(
            &mut heap,
            &mut positions,
            OpenNode {
                node: 1,
                cost: 2,
                estimate: 5,
            },
        ));
        assert!(heap_push_or_decrease(
            &mut heap,
            &mut positions,
            OpenNode {
                node: 2,
                cost: 1,
                estimate: 4,
            },
        ));
        assert_eq!(heap_pop(&mut heap, &mut positions).unwrap().node, 2);
        assert_eq!(heap_pop(&mut heap, &mut positions).unwrap().node, 1);
        assert_eq!(heap_pop(&mut heap, &mut positions).unwrap().node, 3);
    }

    #[test]
    fn decrease_key_does_not_grow_the_heap() {
        let mut heap = Vec::with_capacity(2);
        let mut positions = vec![u32::MAX; 2];
        assert!(heap_push_or_decrease(
            &mut heap,
            &mut positions,
            OpenNode {
                node: 1,
                cost: 10,
                estimate: 20
            },
        ));
        assert!(heap_push_or_decrease(
            &mut heap,
            &mut positions,
            OpenNode {
                node: 1,
                cost: 5,
                estimate: 10
            },
        ));
        assert_eq!(heap.len(), 1);
        assert_eq!(heap_pop(&mut heap, &mut positions).unwrap().cost, 5);
    }
}
