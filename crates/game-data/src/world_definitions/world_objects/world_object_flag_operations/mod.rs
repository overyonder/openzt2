use super::{WorldObjectAffordanceFlags, WorldObjectPropertyFlags};

impl WorldObjectPropertyFlags {
    pub const ANIMAL_USABLE: Self = Self(1 << 0);
    pub const GUEST_USABLE: Self = Self(1 << 1);
    pub const STAFF_ONLY: Self = Self(1 << 2);
    pub const INDOOR: Self = Self(1 << 3);
    pub const OUTDOOR: Self = Self(1 << 4);
    pub const WATER_PLACEABLE: Self = Self(1 << 5);
    pub const WALL_PLACEABLE: Self = Self(1 << 6);
    pub const PATH_REQUIRED: Self = Self(1 << 7);
    pub const DONATION_ACCEPTOR: Self = Self(1 << 8);
    pub const VIEWABLE: Self = Self(1 << 9);
    pub const DELETABLE: Self = Self(1 << 10);
    pub const SAVE_RELEVANT: Self = Self(1 << 11);
    pub const WATER_BOUNDARY: Self = Self(1 << 12);

    const ALL_BITS: u64 = 1 << 0
        | 1 << 1
        | 1 << 2
        | 1 << 3
        | 1 << 4
        | 1 << 5
        | 1 << 6
        | 1 << 7
        | 1 << 8
        | 1 << 9
        | 1 << 10
        | 1 << 11
        | 1 << 12;

    #[must_use]
    pub const fn raw_flag_bits(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn from_raw_flag_bits(flag_bits: u64) -> Option<Self> {
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

impl WorldObjectAffordanceFlags {
    pub const EAT: Self = Self(1 << 0);
    pub const DRINK: Self = Self(1 << 1);
    pub const REST: Self = Self(1 << 2);
    pub const PLAY: Self = Self(1 << 3);
    pub const SHELTER: Self = Self(1 << 4);
    pub const VIEW: Self = Self(1 << 5);
    pub const BUY: Self = Self(1 << 6);
    pub const DONATE: Self = Self(1 << 7);
    pub const LEARN: Self = Self(1 << 8);
    pub const CLEAN: Self = Self(1 << 9);
    pub const REPAIR: Self = Self(1 << 10);
    pub const TREAT: Self = Self(1 << 11);
    pub const TRAIN: Self = Self(1 << 12);
    pub const BOARD: Self = Self(1 << 13);
    pub const DISEMBARK: Self = Self(1 << 14);
    pub const OPERATE: Self = Self(1 << 15);

    const ALL_BITS: u64 = 1 << 0
        | 1 << 1
        | 1 << 2
        | 1 << 3
        | 1 << 4
        | 1 << 5
        | 1 << 6
        | 1 << 7
        | 1 << 8
        | 1 << 9
        | 1 << 10
        | 1 << 11
        | 1 << 12
        | 1 << 13
        | 1 << 14
        | 1 << 15;

    #[must_use]
    pub const fn raw_flag_bits(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn from_raw_flag_bits(flag_bits: u64) -> Option<Self> {
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
