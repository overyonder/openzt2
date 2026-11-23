use bevy::prelude::*;
use openzt2_game_data::{
    AssetId, localization::LocalizationCatalog,
    world_definitions::guest_simulation_definitions::PersonNamePool,
};
use std::collections::BTreeMap;

#[derive(Asset, TypePath, Debug)]
pub struct LocalizationAsset {
    pub(super) localization_catalog: LocalizationCatalog,
    pub(super) person_name_pools: BTreeMap<AssetId, PersonNamePool>,
}

impl LocalizationAsset {
    #[cfg(any(test, feature = "testing"))]
    pub(super) fn from_localization_catalog(localization_catalog: LocalizationCatalog) -> Self {
        Self {
            localization_catalog,
            person_name_pools: BTreeMap::new(),
        }
    }
}
