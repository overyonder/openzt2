use bevy::prelude::*;

use crate::locomotion::fence_navigation_policy::FenceNavigationPolicy;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabBodyCapsule;
use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;

#[cfg(test)]
mod tests;

/// The thickness `zt.exe` gives a wall that authors no `BFGCollisionData width`.
const DEFAULT_WALL_WIDTH_M: f32 = 0.25;

/// Keeps a long animal's body off the fences that block it, as `zt.exe`'s
/// `BFGridCollisionComponent` does (`FUN_006d09fd`): each fence tests the
/// capsule its model authors on the nodes `BFCollisionComponent grid` names
/// against a wall of the fence's thickness (`FUN_004d5738`, `FUN_0073db51`),
/// and moves the animal out along the wall's normal by how far it reaches in.
/// Route planning still moves the animal as a circle of its own radius.
pub(super) fn push_long_animal_bodies_off_fences(
    fences: FenceNavigationPolicy,
    mut animals: Query<(Entity, &NavAgent, &GlobalTransform, &mut Transform), With<AnimalVariant>>,
    children: Query<&Children>,
    capsules: Query<(&GlobalTransform, &PrefabBodyCapsule)>,
) {
    for (animal, agent, global, mut transform) in &mut animals {
        // Capsules carry last frame's placement; move them with the root since.
        let moved = transform.translation - global.translation();
        let mut push = Vec2::ZERO;
        for (start, end, radius, centre) in body_segments(animal, moved, &children, &capsules) {
            let (start, end) = (start + push, end + push);
            for (a, b, width) in fences.blocking_fence_walls(animal, agent.capabilities) {
                let width = if width > 0.0 {
                    width
                } else {
                    DEFAULT_WALL_WIDTH_M
                };
                push += wall_push(start, end, radius, a, b, width * 0.5, centre);
            }
        }
        if push != Vec2::ZERO {
            transform.translation += Vec3::new(push.x, 0.0, push.y);
        }
    }
}

/// Keeps animals' bodies out of each other. `zt.exe`'s grid collision
/// (`FUN_006d09fd`) tests a body against every object in the cells around it,
/// other animals included, so two elephants do not walk into each other's
/// sides. Each of two touching bodies moves out by half how far they overlap,
/// as each makes the same test. Runs before fences push, which win.
pub(super) fn push_animal_bodies_apart(
    mut animals: Query<
        (Entity, &GlobalTransform, &mut Transform),
        (
            With<AnimalVariant>,
            With<NavAgent>,
            // A docked animal, such as a young one nursing, stands where its
            // dock puts it.
            Without<openzt2_zoo_state::locomotion::locomotion_types::Docking>,
        ),
    >,
    children: Query<&Children>,
    capsules: Query<(&GlobalTransform, &PrefabBodyCapsule)>,
) {
    let bodies = animals
        .iter()
        .map(|(animal, global, transform)| {
            let moved = transform.translation - global.translation();
            (animal, body_segments(animal, moved, &children, &capsules))
        })
        .filter(|(_, segments)| !segments.is_empty())
        .collect::<Vec<_>>();
    let pushes = body_pushes(bodies.iter().map(|(_, segments)| segments.as_slice()));
    for ((animal, _), push) in bodies.into_iter().zip(pushes) {
        if push != Vec2::ZERO {
            if let Ok((_, _, mut transform)) = animals.get_mut(animal) {
                transform.translation += Vec3::new(push.x, 0.0, push.y);
            }
        }
    }
}

/// How far to move each of several bodies, given as capsule segments, out of
/// the others: half of each overlap, away from the other body.
fn body_pushes<'a>(bodies: impl Iterator<Item = &'a [BodySegment]> + Clone) -> Vec<Vec2> {
    let bodies = bodies.collect::<Vec<_>>();
    let mut pushes = vec![Vec2::ZERO; bodies.len()];
    for (i, own) in bodies.iter().enumerate() {
        for (j, other) in bodies.iter().enumerate().skip(i + 1) {
            for &(start, end, radius, centre) in *own {
                for &(a, b, other_radius, _) in *other {
                    let push = wall_push(start, end, radius, a, b, other_radius, centre) * 0.5;
                    pushes[i] += push;
                    pushes[j] -= push;
                }
            }
        }
    }
    pushes
}

/// A capsule seen from above: its segment's ends, radius and centre.
type BodySegment = (Vec2, Vec2, f32, Vec2);

/// An animal's body capsules seen from above: each segment's ends, its radius
/// and its centre, moved by how far the animal moved since they were placed.
fn body_segments(
    animal: Entity,
    moved: Vec3,
    children: &Query<&Children>,
    capsules: &Query<(&GlobalTransform, &PrefabBodyCapsule)>,
) -> Vec<BodySegment> {
    children
        .iter_descendants(animal)
        .filter_map(|descendant| {
            let (capsule_global, capsule) = capsules.get(descendant).ok()?;
            let scale = capsule_global.scale().y.abs().max(f32::EPSILON);
            let axis = capsule_global
                .affine()
                .transform_vector3(Vec3::Y)
                .normalize_or_zero()
                * capsule.half_height_m
                * scale;
            let centre = capsule_global.translation() + moved;
            Some((
                (centre - axis).xz(),
                (centre + axis).xz(),
                capsule.radius_m * capsule_global.scale().x.abs(),
                centre.xz(),
            ))
        })
        .collect()
}

/// How far to move a capsule from `start` to `end` of `radius` off the wall
/// from `a` to `b` of half thickness `half_width`, seen from above. A body
/// lying across the wall goes back to the side its `centre` is on.
fn wall_push(
    start: Vec2,
    end: Vec2,
    radius: f32,
    a: Vec2,
    b: Vec2,
    half_width: f32,
    centre: Vec2,
) -> Vec2 {
    let (on_body, on_wall) = closest_points_between_segments(start, end, a, b);
    let reach = radius + half_width;
    let offset = on_body - on_wall;
    let distance = offset.length();
    if distance >= reach {
        return Vec2::ZERO;
    }
    let direction = if distance > 1.0e-4 {
        offset / distance
    } else {
        let normal = (b - a).perp().normalize_or_zero();
        if normal.dot(centre - a) < 0.0 {
            -normal
        } else {
            normal
        }
    };
    direction * (reach - distance)
}

/// The closest points of two segments, on the first and the second.
fn closest_points_between_segments(p0: Vec2, p1: Vec2, q0: Vec2, q1: Vec2) -> (Vec2, Vec2) {
    let (d1, d2, r) = (p1 - p0, q1 - q0, p0 - q0);
    let crossing = d1.perp_dot(d2);
    if crossing.abs() > 1.0e-6 {
        let s = d2.perp_dot(r) / crossing;
        let t = d1.perp_dot(r) / crossing;
        if (0.0..=1.0).contains(&s) && (0.0..=1.0).contains(&t) {
            let point = p0 + d1 * s;
            return (point, point);
        }
    }
    let onto = |point: Vec2, start: Vec2, direction: Vec2| {
        let length = direction.length_squared();
        let along = if length > 0.0 {
            ((point - start).dot(direction) / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        start + direction * along
    };
    [
        (p0, onto(p0, q0, d2)),
        (p1, onto(p1, q0, d2)),
        (onto(q0, p0, d1), q0),
        (onto(q1, p0, d1), q1),
    ]
    .into_iter()
    .min_by(|x, y| {
        x.0.distance_squared(x.1)
            .total_cmp(&y.0.distance_squared(y.1))
    })
    .expect("four candidate pairs")
}
