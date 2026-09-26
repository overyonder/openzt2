use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct ZooEntrance {
    pub(crate) arrival: Vec3,
    pub(crate) inside: Vec3,
    pub(crate) exit: Vec3,
}

/// Gate anchor used by the guest-generation arrival and departure offsets.
pub(super) const fn zoo_entrance_from_authored_gate_anchor(anchor: Vec3) -> ZooEntrance {
    ZooEntrance {
        arrival: anchor,
        inside: anchor,
        exit: anchor,
    }
}

pub(super) fn synchronize_zoo_entrance_anchor_after_transform_changes(
    mut entrances: Query<(&Transform, &mut ZooEntrance), Changed<Transform>>,
) {
    for (transform, mut entrance) in &mut entrances {
        let next = zoo_entrance_from_authored_gate_anchor(transform.translation);
        if *entrance != next {
            *entrance = next;
        }
    }
}
