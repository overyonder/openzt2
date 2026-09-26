use bevy::prelude::*;
use openzt2_game_data::ui_document::action::information::{
    InformationGraphSeries, InformationGraphType,
};

/// Selection state for one graph panel. Rendering reads the selected authored
/// graph series directly and Bevy owns the resulting geometry.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InformationGraph {
    pub(crate) graph: Option<InformationGraphSeries>,
    pub(crate) graph_type: Option<InformationGraphType>,
}
