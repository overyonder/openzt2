use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::{
    UiShellPresentationSlotBinding, UiTranquilizerHeadsUpDisplaySlotBinding,
};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiTranquilizerHudBinding(pub(crate) UiTranquilizerHeadsUpDisplaySlotBinding);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiShellPresentationBinding(pub(crate) UiShellPresentationSlotBinding);
