use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PrefabAuthoredBillboardOrientationMode {
    pub mode: u32,
    pub authored_local_rotation: Quat,
    /// The billboard's content faces local +Z with +Y up (a BFB model),
    /// rather than the source +Y with +Z up (a NIF model).
    pub faces_positive_z: bool,
}

impl PrefabAuthoredBillboardOrientationMode {
    pub fn from_authored_mode_and_local_rotation(mode: u32, authored_local_rotation: Quat) -> Self {
        Self {
            mode,
            authored_local_rotation,
            faces_positive_z: false,
        }
    }

    #[must_use]
    pub fn facing_positive_z(mut self, faces_positive_z: bool) -> Self {
        self.faces_positive_z = faces_positive_z;
        self
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrefabBillboardProjection;
