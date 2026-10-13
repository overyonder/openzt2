use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;

use crate::input::input_types::ActionSource;

#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct UiNodeActivated {
    pub source: ActionSource,
    pub node: Entity,
    pub trigger: UiTrigger,
}
