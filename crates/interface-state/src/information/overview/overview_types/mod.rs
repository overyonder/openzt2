use bevy::prelude::*;
use openzt2_game_data::AssetId;

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
