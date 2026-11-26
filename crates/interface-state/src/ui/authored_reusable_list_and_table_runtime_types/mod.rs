use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::widget_live_collection::UiWidgetLiveCollectionSource, AssetId,
};

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

#[derive(Component, Debug, Clone)]
#[require(RequestedUiListRowCount)]
pub struct UiListPolicy {
    pub row_document: Option<Handle<UiDocumentAsset>>,
    pub opener_node: AssetId,
    pub drop_list_display_node: AssetId,
    pub source: UiWidgetLiveCollectionSource,
}

/// Immutable selector for an authored type-list. Its rows remain in the loaded
/// world catalogue; this component contains only the authored native kind
/// indexes used to select them.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiTypeListPolicy {
    pub included_kinds: Vec<AssetId>,
    pub excluded_kinds: Vec<AssetId>,
    pub filters: Vec<AssetId>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetUiListRowCount {
    pub list: Entity,
    pub count: u16,
}

#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RequestedUiListRowCount(pub Option<u16>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiListRow {
    pub index: u16,
}

/// Classifies authored variable-row surfaces whose immutable row content
/// remains borrowed from the owning UI document through `UiNodeId`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiTablePolicy {
    AdoptionList,
    FinanceLabels,
    FinanceValues,
    WorldMap,
}

/// Authored catalogue kinds whose selection must leave list memory unchanged.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct CatalogueTypeListMemoryPolicy(pub Box<[AssetId]>);

impl CatalogueTypeListMemoryPolicy {
    pub fn remembers(
        &self,
        entry: &openzt2_game_data::world_definitions::catalogue_and_progression::catalogue_definition_types::CatalogueEntry,
    ) -> bool {
        !self
            .0
            .iter()
            .any(|kind| entry.kind == *kind || entry.kinds.contains(kind))
    }
}

/// The authored group fragment and authored rule fragment on a goal list.
#[derive(Component, Debug, Clone)]
pub struct UiScenarioGoalGroupDocument(pub Handle<UiDocumentAsset>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiScenarioGoalRow {
    pub list: Entity,
    pub objective: Entity,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiScenarioGoalGroup {
    pub list: Entity,
    pub group: AssetId,
}
