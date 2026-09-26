use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::{
    UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource,
};
use openzt2_game_data::AssetId;

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

/// Retains the canonical document asset which produced one projected UI root.
#[derive(Component, Debug, Clone)]
pub(crate) struct UiDocumentRoot {
    pub(crate) document: Handle<UiDocumentAsset>,
}

/// Identifies one projected node by its canonical document index and stable id.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiNodeId {
    pub(crate) index: u32,
    pub(crate) id: AssetId,
}

/// Identifies the entity which owns one projected UI document instance.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiDocumentOwner(pub(crate) Entity);

/// Retains the authored source which controls projected visibility.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct UiVisibleBinding(pub(crate) UiBooleanPropertyBindingSource);

/// Retains the authored source which controls an integer value.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct UiValueBinding(pub(crate) UiIntegerPropertyBindingSource);

/// Stores the current integer value of one projected authored UI node.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiValue(pub(crate) i64);
