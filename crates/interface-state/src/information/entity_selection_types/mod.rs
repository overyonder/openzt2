use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::input::input_types::ActionSource;

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
pub struct InfoPanel;

/// The entity carried by a projected information-list row. Consumers use the
/// canonical world entity directly rather than a copied row record.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct InformationEntitySource(pub Entity);

/// The world UI owner selects one inspectable subject.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = SelectedBy)]
pub struct SelectedSubject(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = SelectedSubject)]
pub struct SelectedBy(Vec<Entity>);

#[derive(bevy::ecs::system::SystemParam)]
pub struct WorldUiSelection<'w, 's> {
    subject: Query<
        'w,
        's,
        &'static SelectedSubject,
        With<crate::ui::ui_document_lifecycle_contracts::InGameUiOwner>,
    >,
}

impl WorldUiSelection<'_, '_> {
    pub fn subject(&self) -> Option<Entity> {
        self.subject.single().ok().map(|selection| selection.0)
    }
}
