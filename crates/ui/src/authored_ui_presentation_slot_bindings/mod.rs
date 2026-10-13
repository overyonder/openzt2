use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiShellPresentationSlotBinding;
use openzt2_game_data::ui_document::node_property_binding::UiTranquilizerHeadsUpDisplaySlotBinding;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiTranquilizerHudBinding(pub UiTranquilizerHeadsUpDisplaySlotBinding);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiShellPresentationBinding(pub UiShellPresentationSlotBinding);
