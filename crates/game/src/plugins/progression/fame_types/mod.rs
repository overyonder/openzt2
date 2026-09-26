use bevy::prelude::*;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct Fame {
    pub half_stars: u8,
    pub maximum_reached: u8,
    pub maximum_percent_reached: Option<f32>,
}
