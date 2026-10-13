//! Per-document world-definition assets and precedence-aware lookup indexes.

pub mod behavior_auxiliary_source_document_lowering;
pub mod behavior_auxiliary_source_types;
pub mod lower;
pub mod timing;
pub mod world_definition_asset_loading;
pub mod world_definition_asset_set_refresh_and_indexing;
pub mod world_definition_asset_set_state_and_borrowing_queries;
pub mod world_definition_document_asset_and_demand_loaded_dependency_paths;
pub mod world_definition_load_queue;
pub mod world_definition_source_document_lowering;

use bevy::{asset::AssetApp, prelude::*};

use crate::application_lifecycle::GamePhase;

use self::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use self::{
    world_definition_asset_loading::WorldDefinitionAssetLoader,
    world_definition_asset_set_refresh_and_indexing::{
        index_loaded_world_definition_assets_by_identifier_and_singleton_kind,
        refresh_world_definition_asset_set_after_archive_revision,
    },
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};

pub struct WorldDefinitionAssetPlugin;

impl Plugin for WorldDefinitionAssetPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<WorldDefinitionAsset>()
            .init_asset_loader::<WorldDefinitionAssetLoader>()
            .init_resource::<WorldDefinitions>()
            .add_systems(
                PreUpdate,
                (
                    refresh_world_definition_asset_set_after_archive_revision,
                    index_loaded_world_definition_assets_by_identifier_and_singleton_kind,
                )
                    .chain()
                    .run_if(in_state(GamePhase::Loading).or_else(in_state(GamePhase::InGame))),
            );
    }
}
