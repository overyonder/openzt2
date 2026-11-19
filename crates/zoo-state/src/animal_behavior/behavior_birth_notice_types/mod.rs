use bevy::prelude::*;
use openzt2_game_data::species::Sex;

#[derive(Message, Debug)]
pub struct BehaviorBirthNotice {
    pub subject: Entity,
    pub mother_name: Option<String>,
    pub sex: Option<Sex>,
}
