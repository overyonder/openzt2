//! Per-document species assets and their precedence-aware handle index.

pub mod source;
pub mod species_asset_loading;
pub mod species_asset_set_refresh_and_indexing;
pub mod species_asset_types;

use bevy::{asset::AssetApp, prelude::*};

use crate::application_lifecycle::GamePhase;
use crate::asset_source::ArchiveOverlayChanged;
use crate::world_asset_preparation::WorldAssetPreparation;

use self::{
    species_asset_loading::SpeciesAssetLoader,
    species_asset_set_refresh_and_indexing::{
        discover_species_catalogue_sources,
        index_loaded_species_assets_by_species_and_variant_identifier,
    },
    species_asset_types::{SpeciesAsset, SpeciesAssets},
};

pub struct SpeciesAssetPlugin;

impl Plugin for SpeciesAssetPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<SpeciesAsset>()
            .init_asset_loader::<SpeciesAssetLoader>()
            .init_resource::<SpeciesAssets>()
            .add_message::<ArchiveOverlayChanged>()
            .add_systems(
                PreUpdate,
                (
                    discover_species_catalogue_sources.run_if(
                        condition_changed_to(
                            true,
                            in_state(GamePhase::Loading)
                                .or_else(in_state(GamePhase::InGame))
                                .or_else(any_with_component::<WorldAssetPreparation>),
                        )
                        .or_else(on_message::<ArchiveOverlayChanged>),
                    ),
                    index_loaded_species_assets_by_species_and_variant_identifier,
                )
                    .chain()
                    .after(bevy::asset::AssetTrackingSystems),
            );
    }
}
