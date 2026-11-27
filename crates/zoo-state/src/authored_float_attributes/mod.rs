use std::collections::BTreeMap;

use bevy::prelude::Component;
use openzt2_game_data::AssetId;

#[derive(Component, Default)]
pub struct AuthoredFloatAttributes(BTreeMap<AssetId, f32>);

impl AuthoredFloatAttributes {
    pub fn get(&self, attribute: AssetId) -> Option<f32> {
        self.0.get(&attribute).copied()
    }

    pub fn set(&mut self, attribute: AssetId, value: f32) {
        self.0.insert(attribute, value);
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (AssetId, f32)> + '_ {
        self.0.iter().map(|(&attribute, &value)| (attribute, value))
    }
}
