use std::ops::BitOr;
use std::ops::BitOrAssign;

use bevy::prelude::*;

/// `flags` accumulates every kind of change the chunk has had,
/// whereas `render` holds only what changed since the chunk's mesh and surface
/// image were last rebuilt, which the render refresh takes.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TerrainDirty {
    pub(crate) flags: TerrainDirtyFlags,
    pub(crate) render: Option<TerrainDirtyRegion>,
    pub(crate) revision: u64,
}

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
    pub(super) const HEIGHT: Self = Self(1);
    pub(super) const SURFACE: Self = Self(2);
    pub(super) const WATER: Self = Self(4);
    pub(super) const COLLISION: Self = Self(8);

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
