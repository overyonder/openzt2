use super::ImmersiveModeActionFlags;

impl ImmersiveModeActionFlags {
    pub const MOVE_FORWARD: Self = Self(1 << 0);
    pub const MOVE_BACK: Self = Self(1 << 1);
    pub const STRAFE_LEFT: Self = Self(1 << 2);
    pub const STRAFE_RIGHT: Self = Self(1 << 3);
    pub const LOOK: Self = Self(1 << 4);
    pub const PRIMARY: Self = Self(1 << 5);
    pub const SECONDARY: Self = Self(1 << 6);
    pub const CONFIRM: Self = Self(1 << 7);
    pub const CANCEL: Self = Self(1 << 8);
    pub const ZOOM: Self = Self(1 << 9);
    pub const ROTATE: Self = Self(1 << 10);

    const ALL_BITS: u32 = (1 << 11) - 1;

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
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn contains_any(self, requested_flags: Self) -> bool {
        self.0 & requested_flags.0 != 0
    }
}
