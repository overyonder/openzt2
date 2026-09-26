use bevy::prelude::*;
use openzt2_game_data::localization::LocalizationCatalog;

#[derive(Asset, TypePath, Debug)]
pub(crate) struct LocalizationAsset {
    pub(super) localization_catalog: LocalizationCatalog,
}

impl LocalizationAsset {
    pub(super) fn from_localization_catalog(localization_catalog: LocalizationCatalog) -> Self {
        Self {
            localization_catalog,
        }
    }
}
