use super::{ScenarioRecordFlags, StartingZooSpawnEntityFlags, WorldMapSupportedGameModeFlags};

impl WorldMapSupportedGameModeFlags {
    pub const FREEFORM: Self = Self(1 << 0);
    pub const CHALLENGE: Self = Self(1 << 1);
    pub const CAMPAIGN: Self = Self(1 << 2);
    pub const TUTORIAL: Self = Self(1 << 3);
    #[must_use]
    pub const fn with_additional_flags(self, additional_flags: Self) -> Self {
        Self(self.0 | additional_flags.0)
    }

    #[must_use]
    pub const fn contains_all(self, requested_flags: Self) -> bool {
        self.0 & requested_flags.0 == requested_flags.0
    }
}

impl StartingZooSpawnEntityFlags {
    pub const ACTIVE: Self = Self(1 << 0);
    pub const VISIBLE: Self = Self(1 << 1);
    pub const SAVE_RELEVANT: Self = Self(1 << 2);
    pub const PLAYER_OWNED: Self = Self(1 << 3);
    #[must_use]
    pub const fn with_additional_flags(self, additional_flags: Self) -> Self {
        Self(self.0 | additional_flags.0)
    }

    #[must_use]
    pub const fn contains_all(self, requested_flags: Self) -> bool {
        self.0 & requested_flags.0 == requested_flags.0
    }
}

impl ScenarioRecordFlags {
    pub const TUTORIAL: Self = Self(1 << 0);
    pub const CHALLENGE: Self = Self(1 << 1);
    pub const REPEATABLE: Self = Self(1 << 2);
    pub const ALLOW_PAUSE: Self = Self(1 << 3);
    pub const HIDDEN_UNTIL_UNLOCKED: Self = Self(1 << 4);
    #[must_use]
    pub const fn with_additional_flags(self, additional_flags: Self) -> Self {
        Self(self.0 | additional_flags.0)
    }

    #[must_use]
    pub const fn contains_all(self, requested_flags: Self) -> bool {
        self.0 & requested_flags.0 == requested_flags.0
    }
}
