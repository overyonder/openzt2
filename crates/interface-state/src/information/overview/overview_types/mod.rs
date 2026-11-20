use bevy::prelude::*;
use openzt2_game_data::ui_document::overview_map_presentation::{
    UiMapColorsRecord, UiOverviewCanvas,
};
use openzt2_game_data::AssetId;

/// One authored overview-map layer and its Bevy image projection.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverviewMapCanvas {
    pub surface: Entity,
    pub layer: u32,
    pub source: UiOverviewCanvas,
    pub palette: UiMapColorsRecord,
    pub painted: bool,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportOverviewMap {
    pub document_owner: Entity,
    pub export_control: Entity,
    pub return_control: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = OverviewLegendControls)]
pub struct OverviewLegendLayer(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = OverviewLegendLayer)]
pub struct OverviewLegendControls(Vec<Entity>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = OverviewSubjectMarkers)]
pub struct OverviewMarker {
    pub subject: Entity,
}

#[derive(Component)]
#[relationship_target(relationship = OverviewMarker, linked_spawn)]
pub struct OverviewSubjectMarkers(Vec<Entity>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverviewCameraMarker;
