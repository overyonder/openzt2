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
        for descendant in children.iter_descendants(animal) {
            let Ok((capsule_global, capsule)) = capsules.get(descendant) else {
                continue;
            };
            let scale = capsule_global.scale().y.abs().max(f32::EPSILON);
            let axis = capsule_global
                .affine()
                .transform_vector3(Vec3::Y)
                .normalize_or_zero()
                * capsule.half_height_m
                * scale;
            let centre = capsule_global.translation() + moved;
            let (start, end) = ((centre - axis).xz() + push, (centre + axis).xz() + push);
            let radius = capsule.radius_m * capsule_global.scale().x.abs();
            for (a, b, width) in fences.blocking_fence_walls(animal, agent.capabilities) {
                let width = if width > 0.0 {
                    width
                } else {
                    DEFAULT_WALL_WIDTH_M
                };
                push += wall_push(start, end, radius, a, b, width * 0.5, centre.xz());
            }
        }
        if push != Vec2::ZERO {
            transform.translation += Vec3::new(push.x, 0.0, push.y);
        }
    }
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
