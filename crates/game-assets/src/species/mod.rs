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
        SpeciesCatalogueLoad, discover_species_catalogue_sources,
        index_loaded_species_assets_by_species_and_variant_identifier,
        registered_animal_binder_sources_changed,
    },
    species_asset_types::{SpeciesAsset, SpeciesAssets},
};

pub struct SpeciesAssetPlugin;

impl Plugin for SpeciesAssetPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<SpeciesAsset>()
            .init_asset::<SpeciesCatalogueLoad>()
            .init_asset_loader::<SpeciesAssetLoader>()
            .init_resource::<SpeciesAssets>()
            .add_message::<ArchiveOverlayChanged>()
            .add_systems(
                PreUpdate,
                (
                    discover_species_catalogue_sources.run_if(
                        condition_changed_to(true, species_catalogue_requested).or_eager(
                            registered_animal_binder_sources_changed
                                .and_then(species_catalogue_requested),
                        ),
                    ),
                    index_loaded_species_assets_by_species_and_variant_identifier.run_if(
                        on_message::<AssetEvent<SpeciesCatalogueLoad>>
                            .or_else(on_message::<AssetEvent<SpeciesAsset>>)
                            .or_else(on_message::<bevy::asset::AssetLoadFailedEvent<SpeciesAsset>>),
                    ),
                )
                    .chain()
                    .after(bevy::asset::AssetTrackingSystems),
            );
    }
}

fn species_catalogue_requested(
    state: Res<State<GamePhase>>,
    preparations: Query<(), With<WorldAssetPreparation>>,
) -> bool {
    matches!(state.get(), GamePhase::Loading | GamePhase::InGame) || !preparations.is_empty()
}
