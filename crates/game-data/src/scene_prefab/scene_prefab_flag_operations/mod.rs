use super::{
    ScenePrefabColliderLayerFlags, ScenePrefabEntityFlags, ScenePrefabRenderableVisibilityFlags,
};

impl ScenePrefabEntityFlags {
    pub const VISIBLE: Self = Self(1 << 0);
    pub const ACTIVE: Self = Self(1 << 1);
    pub const SAVE_RELEVANT: Self = Self(1 << 2);
    pub const STATIC: Self = Self(1 << 3);
}

impl std::ops::BitOr for ScenePrefabEntityFlags {
    type Output = Self;

    fn bitor(self, additional_flags: Self) -> Self::Output {
        Self(self.0 | additional_flags.0)
    }
}

impl ScenePrefabRenderableVisibilityFlags {
    pub const VISIBLE: Self = Self(1 << 0);
    pub const CAST_SHADOW: Self = Self(1 << 1);
    pub const RECEIVE_SHADOW: Self = Self(1 << 2);
    pub const REFLECTION_VISIBLE: Self = Self(1 << 3);
}

impl std::ops::BitOr for ScenePrefabRenderableVisibilityFlags {
    type Output = Self;

    fn bitor(self, additional_flags: Self) -> Self::Output {
        Self(self.0 | additional_flags.0)
    }
}

impl ScenePrefabColliderLayerFlags {
    pub const WORLD: Self = Self(1 << 0);
    pub const ANIMAL: Self = Self(1 << 1);
    pub const GUEST: Self = Self(1 << 2);
    pub const STAFF: Self = Self(1 << 3);
    pub const PROJECTILE: Self = Self(1 << 4);
    pub const WATER: Self = Self(1 << 5);
    pub const PLACEMENT: Self = Self(1 << 6);
    pub const ALL_BITS: u16 = Self::WORLD.0
        | Self::ANIMAL.0
        | Self::GUEST.0
        | Self::STAFF.0
        | Self::PROJECTILE.0
        | Self::WATER.0
        | Self::PLACEMENT.0;
}

impl std::ops::BitOr for ScenePrefabColliderLayerFlags {
    type Output = Self;

    fn bitor(self, additional_flags: Self) -> Self::Output {
        Self(self.0 | additional_flags.0)
    }
}
