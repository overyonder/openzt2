use std::collections::BTreeMap;

use bevy::prelude::Component;
use openzt2_game_data::AssetId;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct EntityNameTagCounters {
    values: BTreeMap<AssetId, u32>,
}

impl EntityNameTagCounters {
    pub fn from_saved_entries(entries: impl IntoIterator<Item = (AssetId, u32)>) -> Option<Self> {
        let mut values = BTreeMap::new();
        for (tag, value) in entries {
            if values.insert(tag, value).is_some() {
                return None;
            }
        }
        Some(Self { values })
    }

    pub fn entries(&self) -> impl Iterator<Item = (AssetId, u32)> + '_ {
        self.values.iter().map(|(tag, value)| (*tag, *value))
    }

    pub fn value(&self, tag: AssetId) -> Option<u32> {
        self.values.get(&tag).copied()
    }

    pub fn record_entity_type_tags(&mut self, tags: &[AssetId]) {
        for tag in tags {
            let value = self.values.entry(*tag).or_default();
            *value = value.wrapping_add(1);
        }
    }
}

/// This instance must not repeat its naming registration.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntityNameRegistered;
