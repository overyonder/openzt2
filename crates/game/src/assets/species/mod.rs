//! Per-document species assets and their precedence-aware handle index.

mod source;
mod species_asset_loading;
mod species_asset_set_refresh_and_indexing;
pub(crate) mod species_asset_types;

use bevy::{asset::AssetApp, prelude::*};

use crate::application_lifecycle::GamePhase;

use self::{
    species_asset_loading::SpeciesAssetLoader,
    species_asset_set_refresh_and_indexing::{
        index_loaded_species_assets_by_species_and_variant_identifier,
        refresh_species_asset_set_after_archive_revision,
    },
    species_asset_types::{SpeciesAsset, SpeciesAssets},
};

pub struct SpeciesAssetPlugin;

impl Plugin for SpeciesAssetPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<SpeciesAsset>()
            .init_asset_loader::<SpeciesAssetLoader>()
            .init_resource::<SpeciesAssets>()
            .add_systems(
                PreUpdate,
                (
                    refresh_species_asset_set_after_archive_revision,
                    index_loaded_species_assets_by_species_and_variant_identifier,
                )
                    .chain()
                    .run_if(in_state(GamePhase::Loading).or_else(in_state(GamePhase::InGame))),
            );
    }
}
