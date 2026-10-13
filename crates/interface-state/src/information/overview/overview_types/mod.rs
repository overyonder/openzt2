use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// One authored overview-map layer and its Bevy image projection.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverviewMapCanvas {
    pub surface: Entity,
    pub layer: u32,
    pub painted: bool,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportOverviewMap {
    pub document_owner: Entity,
    pub export_control: Entity,
    pub return_control: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverviewLegendRow {
    pub owner: Entity,
    pub layer: u32,
    pub visible: bool,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OverviewLegendListState {
    pub rows: u16,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverviewMarker {
    pub subject: Entity,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverviewCameraMarker;
