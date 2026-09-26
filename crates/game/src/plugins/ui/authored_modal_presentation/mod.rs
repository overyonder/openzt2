use bevy::prelude::*;

/// Source-authored modal participation for one projected UI node.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiAuthoredModalPresentation(bool);

impl UiAuthoredModalPresentation {
    pub(crate) const fn from_authored_modal_state(modal: bool) -> Self {
        Self(modal)
    }

    pub(crate) const fn is_modal(self) -> bool {
        self.0
    }
}
