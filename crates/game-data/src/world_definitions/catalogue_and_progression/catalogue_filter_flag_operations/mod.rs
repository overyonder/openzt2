use super::catalogue_definition_types::CatalogueFilterFlags;

impl CatalogueFilterFlags {
    pub const PURCHASABLE: Self = Self(1 << 0);
    pub const BUILDABLE: Self = Self(1 << 1);
    pub const ADOPTABLE: Self = Self(1 << 2);
    pub const HIREABLE: Self = Self(1 << 3);
    pub const EXPANSION: Self = Self(1 << 4);
    pub const MODDED: Self = Self(1 << 5);
    pub const HIDDEN_UNTIL_UNLOCKED: Self = Self(1 << 6);

    const ALL_BITS: u16 = 1 << 0 | 1 << 1 | 1 << 2 | 1 << 3 | 1 << 4 | 1 << 5 | 1 << 6;

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
    pub const fn with_additional_flags(self, additional_flags: Self) -> Self {
        Self(self.0 | additional_flags.0)
    }

    #[must_use]
    pub const fn contains_all(self, requested_flags: Self) -> bool {
        self.0 & requested_flags.0 == requested_flags.0
    }
}
