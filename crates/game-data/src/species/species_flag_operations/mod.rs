use super::{SpeciesFlags, SpeciesVariantFlags};

impl SpeciesFlags {
    pub const SWIMS: Self = Self(1 << 2);
    pub const FLIES: Self = Self(1 << 3);
    pub const PREDATOR: Self = Self(1 << 4);
    pub const PREY: Self = Self(1 << 5);
    pub const SOCIAL: Self = Self(1 << 6);
    pub const SOLITARY: Self = Self(1 << 7);
    pub const ADOPTABLE: Self = Self(1 << 8);
    pub const EXTINCT: Self = Self(1 << 9);
    pub const ENDANGERED: Self = Self(1 << 10);
    pub const SUPER: Self = Self(1 << 11);
    pub const SMALL_PREDATOR: Self = Self(1 << 12);
    pub const MEDIUM_PREDATOR: Self = Self(1 << 13);
    pub const LARGE_PREDATOR: Self = Self(1 << 14);
    pub const EXTRA_LARGE_PREDATOR: Self = Self(1 << 15);
    pub const SMALL_PREY: Self = Self(1 << 16);
    pub const MEDIUM_PREY: Self = Self(1 << 17);
    pub const LARGE_PREY: Self = Self(1 << 18);
    pub const EXTRA_LARGE_PREY: Self = Self(1 << 19);
    pub const DOUBLE_EXTRA_LARGE_PREY: Self = Self(1 << 20);

    #[must_use]
    pub const fn contains(self, requested_flags: Self) -> bool {
        self.0 & requested_flags.0 == requested_flags.0
    }
}

impl std::ops::BitOr for SpeciesFlags {
    type Output = Self;

    fn bitor(self, additional_flags: Self) -> Self::Output {
        Self(self.0 | additional_flags.0)
    }
}

impl SpeciesVariantFlags {
    pub const DEFAULT: Self = Self(1 << 0);
    pub const RARE: Self = Self(1 << 1);
    pub const ALBINO: Self = Self(1 << 2);
    pub const STERILE: Self = Self(1 << 3);

    #[must_use]
    pub const fn contains(self, requested_flags: Self) -> bool {
        self.0 & requested_flags.0 == requested_flags.0
    }
}

impl std::ops::BitOr for SpeciesVariantFlags {
    type Output = Self;

    fn bitor(self, additional_flags: Self) -> Self::Output {
        Self(self.0 | additional_flags.0)
    }
}
