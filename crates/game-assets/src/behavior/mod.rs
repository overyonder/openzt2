//! Bevy registration and public crate API for behavior-document assets.

use bevy::{
    asset::{AssetApp, AssetEventSystems, AssetLoadFailedEvent},
    prelude::*,
};

pub mod behavior_asset_loading;
pub mod behavior_asset_types;
pub mod behavior_document_indexing;
pub mod behavior_document_queries;
pub mod source;

use crate::asset_source::ArchiveOverlayChanged;
use behavior_asset_loading::BehaviorDocumentAssetLoader;
use behavior_asset_types::{BehaviorDocumentAsset, LoadedBehaviorDocumentCollection};
use behavior_document_indexing::{discover_behavior_documents, update_behavior_declarations};

pub struct BehaviorDocumentAssetPlugin;

impl Plugin for BehaviorDocumentAssetPlugin {
    fn build(&self, application: &mut App) {
        application
            .init_asset::<BehaviorDocumentAsset>()
            .init_asset_loader::<BehaviorDocumentAssetLoader>()
            .init_resource::<LoadedBehaviorDocumentCollection>()
            .add_message::<ArchiveOverlayChanged>()
            .add_systems(
                PreUpdate,
                discover_behavior_documents.run_if(
                    resource_added::<crate::asset_source::AssetArchives>
                        .or_else(on_message::<ArchiveOverlayChanged>),
                ),
            )
            .add_systems(
                PostUpdate,
                update_behavior_declarations
                    .after(AssetEventSystems)
                    .run_if(
                        on_message::<AssetEvent<BehaviorDocumentAsset>>
                            .or_else(on_message::<AssetLoadFailedEvent<BehaviorDocumentAsset>>),
                    ),
            );
    }
}
