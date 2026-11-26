use std::ops::BitOr;
use std::ops::BitOrAssign;

use bevy::prelude::*;

/// The chunk's height or water samples changed.
#[derive(Component)]
pub(super) struct TerrainDirty;

/// The region still owed to the mesh and base surface refresh.
#[derive(Component, PartialEq, Eq)]
#[component(storage = "SparseSet")]
pub(super) struct TerrainRenderPending(pub(super) TerrainDirtyRegion);

#[derive(Component, PartialEq, Eq)]
#[component(storage = "SparseSet")]
pub(super) struct TerrainSurfaceBasePending(pub(super) TerrainDirtyRegion);

/// Inclusive sample bounds of a change and the kinds of change inside them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TerrainDirtyRegion {
    pub(crate) min: UVec2,
    pub(crate) max: UVec2,
    pub(crate) flags: TerrainDirtyFlags,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(transparent)]
pub(super) struct TerrainDirtyFlags(pub(crate) u8);

impl TerrainDirtyFlags {
    pub(super) const fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub(super) const HEIGHT: Self = Self(1);
    pub(super) const SURFACE: Self = Self(2);
    pub(super) const WATER: Self = Self(4);

    pub(super) const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub(super) const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
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
