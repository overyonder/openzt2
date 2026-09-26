use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;

use crate::plugins::input::input_types::ActionSource;

#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub(crate) struct UiNodeActivated {
    pub(crate) source: ActionSource,
    pub(crate) node: Entity,
    pub(crate) trigger: UiTrigger,
}
