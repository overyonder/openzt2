use bevy::prelude::*;
use openzt2_game_data::ui_document::action::information::{
    InformationGraphSeries, InformationGraphType,
};
use openzt2_game_data::AssetId;

/// Selection state for one graph panel. Rendering reads the selected authored
/// graph series directly and Bevy owns the resulting geometry.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct InformationGraph {
    pub graph: Option<InformationGraphSeries>,
    pub label_document: Option<AssetId>,
    pub graph_type: Option<InformationGraphType>,
}
