use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::{
    UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource,
};
use openzt2_game_data::AssetId;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

/// Retains the canonical document asset which produced one projected UI root.
#[derive(Component, Debug, Clone)]
pub struct UiDocumentRoot {
    pub document: Handle<UiDocumentAsset>,
}

/// Identifies one projected node by its canonical document index and stable id.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiNodeId {
    pub index: u32,
    pub id: AssetId,
}

/// Identifies the entity which owns one projected UI document instance.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiDocumentOwner(pub Entity);

/// Retains the authored source which controls projected visibility.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiVisibleBinding(pub UiBooleanPropertyBindingSource);

/// Retains the authored source which controls an integer value.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiValueBinding(pub UiIntegerPropertyBindingSource);

/// Stores the current integer value of one projected authored UI node.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiValue(pub i64);
