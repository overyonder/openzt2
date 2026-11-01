use bevy::prelude::*;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct Fame {
    pub half_stars: u8,
    pub current_percent: Option<f32>,
    pub maximum_reached: u8,
    pub maximum_percent_reached: Option<f32>,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct SetScenarioMaximumFame {
    pub percent: f32,
}

impl Fame {
    pub fn half_stars_for_percent(percent: f32) -> u8 {
        (percent / 10.0).floor() as u8
    }
}
