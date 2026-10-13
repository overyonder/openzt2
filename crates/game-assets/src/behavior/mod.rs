//! Bevy registration and public crate API for behavior-document assets.

use bevy::{asset::AssetApp, prelude::*};

pub mod behavior_asset_loading;
pub mod behavior_asset_types;
pub mod behavior_document_indexing;
pub mod behavior_document_queries;
pub mod source;

use crate::application_lifecycle::GamePhase;
use behavior_asset_loading::BehaviorDocumentAssetLoader;
use behavior_asset_types::{BehaviorDocumentAsset, LoadedBehaviorDocumentCollection};
use behavior_document_indexing::{
    rebuild_behavior_declaration_indexes_after_asset_change,
    refresh_behavior_documents_after_archive_revision_change,
};

pub struct BehaviorDocumentAssetPlugin;

impl Plugin for BehaviorDocumentAssetPlugin {
    fn build(&self, application: &mut App) {
        application
            .init_asset::<BehaviorDocumentAsset>()
            .init_asset_loader::<BehaviorDocumentAssetLoader>()
            .init_resource::<LoadedBehaviorDocumentCollection>()
            .add_systems(
                PreUpdate,
                (
                    refresh_behavior_documents_after_archive_revision_change,
                    rebuild_behavior_declaration_indexes_after_asset_change,
                )
                    .chain()
                    .run_if(in_state(GamePhase::Loading).or_else(in_state(GamePhase::InGame))),
            );
    }
}
