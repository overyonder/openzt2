use super::UiNodeFlags;

impl UiNodeFlags {
    pub const VISIBLE: Self = Self(1 << 0);
    pub const ENABLED: Self = Self(1 << 1);
    pub const CLIP_CHILDREN: Self = Self(1 << 2);
    pub const MODAL: Self = Self(1 << 3);
    pub const FOCUSABLE: Self = Self(1 << 4);
    pub const POINTER_BLOCKING: Self = Self(1 << 5);
    pub const USE_LIST_BOX_COLOR: Self = Self(1 << 6);
    pub const NOTIFY_ANIMATION_COMPLETED: Self = Self(1 << 7);
    /// Authored main-menu surface for optional online messages.
    pub const ONLINE_MESSAGE_SURFACE: Self = Self(1 << 8);
    /// Authored list which owns projected online-message rows.
    pub const ONLINE_MESSAGE_LIST: Self = Self(1 << 9);
    /// Authored row field which receives online-message prose.
    pub const ONLINE_MESSAGE_TEXT: Self = Self(1 << 10);
    /// Authored row field reserved for an optional message icon.
    pub const ONLINE_MESSAGE_ICON: Self = Self(1 << 11);
    /// Authored in-game HUD list that owns feedback-alert fragments.
    pub const FEEDBACK_ALERT_LIST: Self = Self(1 << 12);
    /// Authored alert-fragment button that selects the alert subject.
    pub const FEEDBACK_ALERT_SELECTOR: Self = Self(1 << 13);
    /// Authored alert-fragment text field.
    pub const FEEDBACK_ALERT_TEXT: Self = Self(1 << 14);
    /// Authored alert-fragment subject icon.
    pub const FEEDBACK_ALERT_ICON: Self = Self(1 << 15);
    pub const ALL: u16 = u16::MAX;

    #[must_use]
    pub const fn contains(self, requested_flags: Self) -> bool {
        self.0 & requested_flags.0 == requested_flags.0
    }
}

impl std::ops::BitOr for UiNodeFlags {
    type Output = Self;

    fn bitor(self, additional_flags: Self) -> Self::Output {
        Self(self.0 | additional_flags.0)
    }
}
