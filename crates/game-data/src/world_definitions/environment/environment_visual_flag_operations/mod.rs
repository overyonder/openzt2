use super::EnvironmentVisualFlags;

impl EnvironmentVisualFlags {
    pub const EMPTY: Self = Self(0);
    pub const CLEAR_DEPTH: Self = Self(1 << 0);
    pub const USE_WORLD_LIGHTS: Self = Self(1 << 1);
    pub const SMOOTH_UPDATE: Self = Self(1 << 2);

    #[must_use]
    pub const fn with_additional_flags(self, additional_flags: Self) -> Self {
        Self(self.0 | additional_flags.0)
    }

    #[must_use]
    pub const fn contains_all(self, requested_flags: Self) -> bool {
        self.0 & requested_flags.0 == requested_flags.0
    }
}
