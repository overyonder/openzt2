use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// One authored overview-map layer and its Bevy image projection.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OverviewMapCanvas {
    pub(super) surface: Entity,
    pub(super) layer: u32,
    pub(super) painted: bool,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExportOverviewMap {
    pub(crate) document_owner: Entity,
    pub(crate) export_control: Entity,
    pub(crate) return_control: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::plugins::information) struct OverviewLegendRow {
    pub(super) owner: Entity,
    pub(super) layer: u32,
    pub(super) visible: bool,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::plugins::information) struct OverviewLegendListState {
    pub(super) rows: u16,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::plugins::information) struct OverviewMarker {
    pub(super) subject: Entity,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::plugins::information) struct OverviewCameraMarker;
