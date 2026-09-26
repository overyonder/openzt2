use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::widget_live_collection::UiWidgetLiveCollectionSource, AssetId,
};

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

#[derive(Component, Debug, Clone)]
pub(crate) struct UiListPolicy {
    pub(crate) row_document: Option<Handle<UiDocumentAsset>>,
    pub(crate) opener_node: AssetId,
    pub(crate) drop_list_display_node: AssetId,
    pub(crate) source: UiWidgetLiveCollectionSource,
}

/// Immutable selector for an authored type-list. Its rows remain in the loaded
/// world catalogue; this component contains only the authored native kind
/// indexes used to select them.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct UiTypeListPolicy {
    pub(crate) included_kinds: Vec<AssetId>,
    pub(crate) excluded_kinds: Vec<AssetId>,
    pub(crate) filters: Vec<AssetId>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SetUiListRowCount {
    pub(crate) list: Entity,
    pub(crate) count: u16,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiListRow {
    pub(crate) list: Entity,
    pub(crate) index: u16,
}

/// Classifies authored variable-row surfaces whose immutable row content
/// remains borrowed from the owning UI document through `UiNodeId`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiTablePolicy {
    AdoptionList,
    FinanceLabels,
    FinanceValues,
    WorldMap,
}
