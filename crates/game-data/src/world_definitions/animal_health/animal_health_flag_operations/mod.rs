use super::TranquilizerEligibility;

impl TranquilizerEligibility {
    pub const ESCAPED: Self = Self(1 << 0);
    pub const RAMPAGING: Self = Self(1 << 1);
    pub const CONTAINED: Self = Self(1 << 2);

    const ALL_KNOWN_FLAG_BITS: u8 = 1 << 0 | 1 << 1 | 1 << 2;

    #[must_use]
    pub const fn raw_flag_bits(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn from_flag_bits(flag_bits: u8) -> Option<Self> {
        if flag_bits & !Self::ALL_KNOWN_FLAG_BITS == 0 {
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

impl TryFrom<u8> for TranquilizerEligibility {
    type Error = &'static str;

    fn try_from(flag_bits: u8) -> Result<Self, Self::Error> {
        Self::from_flag_bits(flag_bits).ok_or("tranquilizer eligibility contains unknown flag bits")
    }
}

impl From<TranquilizerEligibility> for u8 {
    fn from(eligibility: TranquilizerEligibility) -> Self {
        eligibility.raw_flag_bits()
    }
}
