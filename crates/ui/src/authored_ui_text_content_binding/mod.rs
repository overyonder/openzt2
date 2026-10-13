use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiTextPropertyBindingSource;

/// Identifies the authored source which supplies one projected
/// Bevy text value. Domain presenters update ordinary `Text` directly.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiTextBinding(pub UiTextPropertyBindingSource);
