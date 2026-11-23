//! Per-document world-definition assets and precedence-aware lookup indexes.

pub mod behavior_auxiliary_source_document_lowering;
pub mod behavior_auxiliary_source_types;
pub mod ground_path_surface_images;
pub mod lower;
pub mod path_placement_highlight_asset;
mod path_placement_highlight_loading;
pub mod timing;
pub mod world_definition_asset_loading;
pub mod world_definition_asset_set_refresh_and_indexing;
pub mod world_definition_asset_set_state_and_borrowing_queries;
pub mod world_definition_document_asset_and_demand_loaded_dependency_paths;
pub mod world_definition_load_queue;
pub mod world_definition_source_document_lowering;

use bevy::{asset::AssetApp, prelude::*};

use crate::application_lifecycle::GamePhase;
use crate::asset_source::ArchiveOverlayChanged;
use crate::world_asset_preparation::WorldAssetPreparation;

use self::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use self::{
    world_definition_asset_loading::WorldDefinitionAssetLoader,
    world_definition_asset_set_refresh_and_indexing::{
        discover_world_definition_catalogue_sources,
        index_loaded_world_definition_assets_by_identifier_and_singleton_kind,
        saved_world_economy_asset::{SavedWorldEconomyAsset, SavedWorldEconomyAssetLoader},
    },
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};

#[derive(SystemSet, Debug, Hash, Eq, PartialEq, Clone)]
pub struct WorldDefinitionPublicationSet;

pub struct WorldDefinitionAssetPlugin;

impl Plugin for WorldDefinitionAssetPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<WorldDefinitionAsset>()
            .init_asset::<world_definition_load_queue::WorldDefinitionLoadCompletion>()
            .init_asset::<SavedWorldEconomyAsset>()
            .init_asset_loader::<SavedWorldEconomyAssetLoader>()
            .init_asset::<ground_path_surface_images::GroundPathSurfaceImage>()
            .init_asset_loader::<ground_path_surface_images::GroundPathSurfaceImageLoader>()
            .init_asset::<path_placement_highlight_asset::PathPlacementHighlightAsset>()
            .init_asset_loader::<path_placement_highlight_loading::PathPlacementHighlightLoader>()
            .init_asset_loader::<WorldDefinitionAssetLoader>()
            .init_resource::<WorldDefinitions>()
            .add_message::<ArchiveOverlayChanged>()
            .add_systems(
                PreUpdate,
                (
                    discover_world_definition_catalogue_sources.run_if(
                        condition_changed_to(
                            true,
                            in_state(GamePhase::Loading)
                                .or_else(in_state(GamePhase::InGame))
                                .or_else(any_with_component::<WorldAssetPreparation>),
                        )
                        .or_else(on_message::<ArchiveOverlayChanged>),
                    ),
                    index_loaded_world_definition_assets_by_identifier_and_singleton_kind
                        .in_set(WorldDefinitionPublicationSet)
                        .run_if(
                            on_message::<
                                bevy::asset::AssetEvent<
                                    world_definition_load_queue::WorldDefinitionLoadCompletion,
                                >,
                            >
                                .or_else(
                                    on_message::<bevy::asset::AssetEvent<WorldDefinitionAsset>>,
                                )
                                .or_else(
                                    on_message::<
                                        bevy::asset::AssetLoadFailedEvent<WorldDefinitionAsset>,
                                    >,
                                ),
                        ),
                )
                    .chain()
                    .after(bevy::asset::AssetTrackingSystems),
            );
    }
}
