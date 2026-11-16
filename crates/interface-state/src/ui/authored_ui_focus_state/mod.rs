use bevy::prelude::*;

/// Focus state owned by one projected authored UI document.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct UiFocusScope {
    pub focused: Option<Entity>,
    pub default: Option<Entity>,
}

/// Source-authored keyboard and controller focus policy for one UI node.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiFocusable {
    pub order: u32,
    pub enabled: bool,
}

/// Presentation-only focus state for keyboard/controller navigation. Pointer
/// hover is resolved separately by the alpha-aware picker.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiFocusPresentation(pub bool);
