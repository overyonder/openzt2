use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct PrefabAuthoredBillboardOrientationMode {
    pub(crate) mode: u32,
    pub(crate) authored_local_rotation: Quat,
}

impl PrefabAuthoredBillboardOrientationMode {
    pub(crate) fn from_authored_mode_and_local_rotation(
        mode: u32,
        authored_local_rotation: Quat,
    ) -> Self {
        Self {
            mode,
            authored_local_rotation,
        }
    }
}
