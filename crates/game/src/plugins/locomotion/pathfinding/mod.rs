use bevy::prelude::*;

use super::{
    locomotion_types::{
        LocomotionMode, NavAgent, NavFlags, NavigationFailure, NavigationOverlay, OpenNode, Route,
        RouteScratch,
    },
    terrain_derived_navigation_graph_types::{
        TerrainDerivedNavigationGraph,
        TerrainDerivedNavigationGraphEdgeFlags as NavigationGraphEdgeFlags,
        TerrainDerivedNavigationGraphTraversalKind,
    },
};

pub(crate) fn plan_route(
    navigation: &TerrainDerivedNavigationGraph,
    overlay: &NavigationOverlay,
    agent: &NavAgent,
    mode: LocomotionMode,
    start_world: Vec3,
    destination: Vec3,
    route: &mut Route,
    scratch: &mut RouteScratch,
) -> Result<(), NavigationFailure> {
    if !start_world.is_finite() || !destination.is_finite() {
        return Err(NavigationFailure::OutsideWorld);
    }
    let start =
        nearest_eligible(navigation, agent, start_world).ok_or(NavigationFailure::NoStartNode)?;
    let goal =
        nearest_eligible(navigation, agent, destination).ok_or(NavigationFailure::OutsideWorld)?;
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
                || overlay.edge_blocked(current.node, next)
                || edge.edge_flags.0 & NavigationGraphEdgeFlags::DISABLED_BY_DEFAULT.0 != 0
                || !traversal_allowed(edge.traversal_kind, mode)
            {
                continue;
            }
            let Some(next_node) = navigation.find_navigation_node(next) else {
                continue;
            };
            if next_node.eligibility_flags.0 & agent.capabilities.0 == 0
                || f32::from(next_node.clearance_centimetres) * 0.01 < agent.radius_m
                || f32::from(edge.width_centimetres) * 0.01 < agent.radius_m * 2.0
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
    for node in scratch.reverse_path.iter().rev().skip(1) {
        push_if_turn(navigation_position(navigation, *node), &mut route.points);
    }
    push_if_turn(destination, &mut route.points);
    Ok(())
}

fn nearest_eligible(
    navigation: &TerrainDerivedNavigationGraph,
    agent: &NavAgent,
    world: Vec3,
) -> Option<u32> {
    let position = metres_to_cm(world)?;
    navigation
        .find_candidate_node_indices_in_containing_terrain_tile(position)
        .filter(|index| {
            navigation.find_navigation_node(*index).is_some_and(|node| {
                node.eligibility_flags.0 & agent.capabilities.0 != 0
                    && node.eligibility_flags.0 & NavFlags::DISABLED_BY_DEFAULT.0 == 0
                    && f32::from(node.clearance_centimetres) * 0.01 >= agent.radius_m
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

fn traversal_allowed(
    traversal: TerrainDerivedNavigationGraphTraversalKind,
    mode: LocomotionMode,
) -> bool {
    match traversal {
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
