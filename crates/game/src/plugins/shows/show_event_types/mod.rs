use bevy::prelude::*;
use openzt2_game_data::behavior::action::show_event::BehaviorShowEventAction;

/// A task's registered event, before stage routing and audience arbitration.
/// Registration does not establish a scored or completed performance.
#[derive(Component)]
pub(crate) struct AuthoredAnimalShowEvent {
    pub(crate) performer: Entity,
    pub(crate) execution_id: u64,
    pub(crate) policy: BehaviorShowEventAction,
}
