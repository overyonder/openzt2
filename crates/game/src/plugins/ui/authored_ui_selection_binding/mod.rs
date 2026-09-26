use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiBooleanPropertyBindingSource;

/// Marks a projected selection value whose canonical source belongs to a
/// domain presenter rather than authored toggle interaction.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct UiSelectedBinding(pub(crate) UiBooleanPropertyBindingSource);
