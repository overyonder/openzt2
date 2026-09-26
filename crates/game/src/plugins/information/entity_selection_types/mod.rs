use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::input::input_types::ActionSource;

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SelectedEntity(pub(crate) Option<Entity>);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SelectionRequest {
    pub(crate) entity: Option<Entity>,
    pub(crate) source: ActionSource,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SelectionChanged {
    pub(crate) previous: Option<Entity>,
    pub(crate) current: Option<Entity>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Inspectable {
    pub(crate) definition: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InfoPanel {
    pub(crate) subject: Entity,
}

/// The entity carried by a projected information-list row. Consumers use the
/// canonical world entity directly rather than a copied row record.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InformationEntitySource(pub(crate) Entity);
