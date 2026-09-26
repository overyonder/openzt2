use super::FenceTraversalBlockingFlags;

impl FenceTraversalBlockingFlags {
    pub const GUEST: Self = Self(1 << 0);
    pub const STAFF: Self = Self(1 << 1);
    pub const ANIMAL: Self = Self(1 << 2);
    pub const VEHICLE: Self = Self(1 << 3);
    pub const AIR: Self = Self(1 << 4);
    pub const WATER: Self = Self(1 << 5);

    const ALL_BITS: u16 = 1 << 0 | 1 << 1 | 1 << 2 | 1 << 3 | 1 << 4 | 1 << 5;

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
