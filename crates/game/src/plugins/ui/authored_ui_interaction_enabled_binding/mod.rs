use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiBooleanPropertyBindingSource;

/// Identifies the authored source which controls whether one
/// projected UI node may accept interaction.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct UiEnabledBinding(pub(crate) UiBooleanPropertyBindingSource);
