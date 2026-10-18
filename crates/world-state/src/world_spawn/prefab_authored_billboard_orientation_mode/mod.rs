use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PrefabAuthoredBillboardOrientationMode {
    pub mode: u32,
    pub authored_local_rotation: Quat,
}

impl PrefabAuthoredBillboardOrientationMode {
    pub fn from_authored_mode_and_local_rotation(mode: u32, authored_local_rotation: Quat) -> Self {
        Self {
            mode,
            authored_local_rotation,
        }
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrefabBillboardProjection;
