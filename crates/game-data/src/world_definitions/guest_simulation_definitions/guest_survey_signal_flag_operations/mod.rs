use super::GuestNeedSurveySignals;

impl GuestNeedSurveySignals {
    pub const EMPTY: Self = Self(0);
    pub const HUNGER: Self = Self(1 << 0);
    pub const THIRST: Self = Self(1 << 1);
    pub const DESSERT: Self = Self(1 << 2);
    pub const GIFT: Self = Self(1 << 3);
    pub const BATHROOM: Self = Self(1 << 4);

    #[must_use]
    pub const fn with_additional_signals(self, additional_signals: Self) -> Self {
        Self(self.0 | additional_signals.0)
    }

    #[must_use]
    pub const fn contains_all(self, requested_signals: Self) -> bool {
        self.0 & requested_signals.0 == requested_signals.0
    }
}
