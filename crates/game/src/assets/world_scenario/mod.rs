//! World-scenario asset loading, precedence indexing and public borrowing queries.

mod authored;
mod starting_zoo;
mod world_scenario_asset_set_refresh_and_indexing;
pub mod world_scenario_asset_set_state_and_borrowing_queries;
pub mod world_scenario_document_asset_and_dependency_handles;
mod world_scenario_document_loading;
pub(crate) mod world_scenario_source_reference_resolution;

use bevy::{asset::AssetApp, prelude::*};

use self::{
    world_scenario_asset_set_refresh_and_indexing::{
        index_loaded_world_scenario_assets_by_identifier,
        refresh_world_scenario_asset_set_after_archive_revision,
    },
    world_scenario_document_loading::WorldScenarioDocumentLoader,
};
use self::{
    world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios,
    world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
};

pub struct WorldScenarioDocumentAssetPlugin;

impl Plugin for WorldScenarioDocumentAssetPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<WorldScenarioDocumentAsset>()
            .init_asset_loader::<WorldScenarioDocumentLoader>()
            .init_resource::<WorldScenarios>()
            .add_systems(
                PreUpdate,
                (
                    refresh_world_scenario_asset_set_after_archive_revision,
                    index_loaded_world_scenario_assets_by_identifier,
                )
                    .chain(),
            );
    }
}
