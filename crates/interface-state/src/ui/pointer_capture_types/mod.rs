use bevy::prelude::*;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum UiWheelCapture {
    #[default]
    World,
    Blocked,
    Scroll(Entity),
}

/// Whether the mouse currently resolves to an authored UI surface.
///
/// Alpha-aware picking is the sole writer. World interaction systems read this
/// instead of independently interpreting Bevy's raw hover stack, so visual
/// hover, button activation, and world-click capture share one result.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct UiPointerCapture {
    pub over_ui: bool,
    pub target: Option<Entity>,
    pub wheel: UiWheelCapture,
}
