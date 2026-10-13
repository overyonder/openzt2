use bevy::{ecs::system::SystemParam, prelude::*};

use crate::plugins::{
    photos::{
        photo_album_types::{
            DefaultPhotoAlbumStorageInitializedForWorld, DEFAULT_PHOTO_ALBUM_NAME_KEY,
        },
        photo_challenge_types::PhotoChallengesInitializedForScenarioRevision,
    },
    world_spawn::{
        persistent_id_types::PersistentIdAllocator,
        world_load_completion_marker::WorldLoadCompleted, world_membership_types::WorldRoot,
    },
};
use openzt2_game_assets::{
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    world_scenario::{
        world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios,
        world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
    },
};

use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure;

#[derive(SystemParam)]
pub(super) struct PhotoSnapshotBaselineReadiness<'w, 's> {
    server: Res<'w, AssetServer>,
    scenarios: Res<'w, WorldScenarios>,
    scenario_assets: Res<'w, Assets<WorldScenarioDocumentAsset>>,
    localization: Res<'w, LocalizationPrecedenceIndex>,
    localization_assets: Res<'w, Assets<LocalizationAsset>>,
    roots: Query<
        'w,
        's,
        (
            Option<&'static PhotoChallengesInitializedForScenarioRevision>,
            Option<&'static DefaultPhotoAlbumStorageInitializedForWorld>,
        ),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
}

impl PhotoSnapshotBaselineReadiness<'_, '_> {
    pub(super) fn is_ready_for_snapshot_application(
        &self,
        root: Entity,
        allocator: &PersistentIdAllocator,
    ) -> Result<bool, WorldSnapshotPersistenceFailure> {
        if self
            .scenarios
            .catalogue_document_load_has_failed(&self.server, &self.scenario_assets)
        {
            return Err(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier);
        }
        if !self
            .scenarios
            .catalogue_documents_are_loaded(&self.scenario_assets)
        {
            return if self
                .scenarios
                .has_pending_document_loads(&self.server, &self.scenario_assets)
            {
                Ok(false)
            } else {
                Err(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)
            };
        }
        if self.scenarios.get(&self.scenario_assets).is_none() {
            return Err(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier);
        }
        let album_name = self
            .localization
            .borrow_loaded_localization_view(&self.localization_assets)
            .and_then(|view| {
                view.find_plain_localized_text(openzt2_game_data::AssetId::from_key(
                    DEFAULT_PHOTO_ALBUM_NAME_KEY,
                ))
            });
        if album_name.is_none() {
            return if self
                .localization
                .has_pending_catalogue_loads(&self.server, &self.localization_assets)
            {
                Ok(false)
            } else {
                Err(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)
            };
        }
        if !allocator.owns_world(root) {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        }
        let Ok((challenges, storage)) = self.roots.get(root) else {
            return Ok(false);
        };
        if storage.is_none() && allocator.next() == u64::MAX {
            return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
        }
        Ok(storage.is_some()
            && self.scenarios.revision().is_some_and(|revision| {
                challenges.is_some_and(|initialized| initialized.matches_revision(revision))
            }))
    }
}
