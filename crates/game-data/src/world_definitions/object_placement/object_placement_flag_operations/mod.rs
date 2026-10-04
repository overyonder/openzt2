use super::{FootprintCellFlags, PlacementConstraints};

impl PlacementConstraints {
    pub const EMPTY: Self = Self(0);
    pub const REQUIRE_PATH: Self = Self(1 << 0);
    pub const REQUIRE_HABITAT: Self = Self(1 << 1);
    pub const REQUIRE_WATER: Self = Self(1 << 2);
    pub const REQUIRE_LAND: Self = Self(1 << 3);
    pub const REQUIRE_WALL: Self = Self(1 << 4);
    pub const REQUIRE_FLAT: Self = Self(1 << 5);
    pub const ALLOW_OVERLAP_SCENERY: Self = Self(1 << 6);
    pub const FLATTEN_TERRAIN_TO_PLACEMENT_HEIGHT: Self = Self(1 << 8);

    const ALL_BITS: u32 = 1 << 0 | 1 << 1 | 1 << 2 | 1 << 3 | 1 << 4 | 1 << 5 | 1 << 6 | 1 << 8;

    #[must_use]
    pub const fn raw_flag_bits(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn from_raw_flag_bits(flag_bits: u32) -> Option<Self> {
        if flag_bits & !Self::ALL_BITS == 0 {
            Some(Self(flag_bits))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn with_additional_flags(self, additional_flags: Self) -> Self {
        Self(self.0 | additional_flags.0)
    }

    #[must_use]
    pub const fn contains_any(self, requested_flags: Self) -> bool {
        self.0 & requested_flags.0 != 0
    }

    #[must_use]
    pub const fn contains_all(self, requested_flags: Self) -> bool {
        self.0 & requested_flags.0 == requested_flags.0
    }
}

impl FootprintCellFlags {
    pub const EMPTY: Self = Self(0);
    pub const OCCUPIED: Self = Self(1 << 0);
    pub const WALKABLE: Self = Self(1 << 1);
    pub const ENTRANCE: Self = Self(1 << 2);
    pub const WATER: Self = Self(1 << 3);
    pub const FOUNDATION_REQUIRED: Self = Self(1 << 4);

    const ALL_BITS: u16 = 1 << 0 | 1 << 1 | 1 << 2 | 1 << 3 | 1 << 4;

    #[must_use]
    pub const fn raw_flag_bits(self) -> u16 {
        self.0
    }

    #[must_use]
    pub const fn from_raw_flag_bits(flag_bits: u16) -> Option<Self> {
        if flag_bits & !Self::ALL_BITS == 0 {
            Some(Self(flag_bits))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn contains_all(self, requested_flags: Self) -> bool {
        self.0 & requested_flags.0 == requested_flags.0
    }
}
