use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::input::input_types::ActionSource;

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SelectedEntity(pub Option<Entity>);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionRequest {
    pub entity: Option<Entity>,
    pub source: ActionSource,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionChanged {
    pub previous: Option<Entity>,
    pub current: Option<Entity>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inspectable {
    pub definition: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct InfoPanel {
    pub subject: Entity,
}

/// The entity carried by a projected information-list row. Consumers use the
/// canonical world entity directly rather than a copied row record.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct InformationEntitySource(pub Entity);
