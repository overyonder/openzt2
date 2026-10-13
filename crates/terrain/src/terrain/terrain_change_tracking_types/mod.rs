use std::ops::BitOr;
use std::ops::BitOrAssign;

use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TerrainDirty {
    pub(crate) min: UVec2,
    pub(crate) max: UVec2,
    pub(crate) flags: TerrainDirtyFlags,
    pub(crate) revision: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(transparent)]
pub(super) struct TerrainDirtyFlags(pub(crate) u8);

impl TerrainDirtyFlags {
    pub(super) const HEIGHT: Self = Self(1);
    pub(super) const SURFACE: Self = Self(2);
    pub(super) const WATER: Self = Self(4);
    pub(super) const COLLISION: Self = Self(8);

    pub(super) const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for TerrainDirtyFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for TerrainDirtyFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}
