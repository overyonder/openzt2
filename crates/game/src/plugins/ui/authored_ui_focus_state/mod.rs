use bevy::prelude::*;

/// Focus state owned by one projected authored UI document.
#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct UiFocusScope {
    pub(crate) focused: Option<Entity>,
    pub(crate) default: Option<Entity>,
}

/// Source-authored keyboard and controller focus policy for one UI node.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiFocusable {
    pub(crate) order: u32,
    pub(crate) enabled: bool,
}

/// Presentation-only focus state for keyboard/controller navigation. Pointer
/// hover remains owned exclusively by Bevy's `Interaction` component.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct UiFocusPresentation(pub(crate) bool);
