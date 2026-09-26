use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiImagePropertyBindingSource;

/// Identifies the authored source which supplies one projected
/// Bevy image. Domain presentation owners update the ordinary `ImageNode`
/// directly and retain no copied image record.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct UiImageBinding(pub(crate) UiImagePropertyBindingSource);
