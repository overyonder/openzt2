use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ZooEntrance {
    pub arrival: Vec3,
    pub inside: Vec3,
    pub exit: Vec3,
}

/// Gate anchor used by the guest-generation arrival and departure offsets.
pub const fn zoo_entrance_from_authored_gate_anchor(anchor: Vec3) -> ZooEntrance {
    ZooEntrance {
        arrival: anchor,
        inside: anchor,
        exit: anchor,
    }
}
