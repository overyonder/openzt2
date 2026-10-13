use bevy::prelude::*;
use openzt2_game_data::ui_document::document::UiDocumentRole;

use crate::plugins::{
    persistence::{
        persistence_failure_types::WorldSnapshotPersistenceFailed,
        persistence_ui_types::OpenLoadSlotCatalogueAfterWorldSnapshotSave,
        save_slot_types::{SaveSlotId, WorldSnapshotSavedToSlot},
    },
    ui::{
        authored_ui_activation_contracts::UiNodeActivated,
        ui_document_asset_load_failure::UiDocumentAssetLoadFailed,
        ui_document_lifecycle_contracts::ShowUiRole, UiSet,
    },
};
use openzt2_game_assets::ui_document::{
    ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    ui_document_role_asset_path_selection::select_bevy_asset_path_for_canonical_ui_document_role,
};

use super::continue_to_load_slot_catalogue_after_world_snapshot_save;

#[test]
fn failed_save_receiver_cannot_continue_after_a_later_save_for_other_owners() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<UiDocumentAsset>()
        .add_message::<UiNodeActivated>()
        .add_message::<UiDocumentAssetLoadFailed>()
        .add_message::<WorldSnapshotPersistenceFailed>()
        .add_message::<WorldSnapshotSavedToSlot>()
        .add_message::<ShowUiRole>()
        .add_systems(
            Update,
            continue_to_load_slot_catalogue_after_world_snapshot_save.after(UiSet::Projection),
        );
    let failed_save_owner = app
        .world_mut()
        .spawn(OpenLoadSlotCatalogueAfterWorldSnapshotSave)
        .id();
    let unrelated_document_owner = app
        .world_mut()
        .spawn(OpenLoadSlotCatalogueAfterWorldSnapshotSave)
        .id();
    let unaffected_owner = app
        .world_mut()
        .spawn(OpenLoadSlotCatalogueAfterWorldSnapshotSave)
        .id();
    let foreign_owner = app.world_mut().spawn_empty().id();
    // Path-bearing handles use the same canonical role selection as projection.
    // This fixture controls the failure/completion messages, not asset-loader timing.
    let save_document = app
        .world()
        .resource::<AssetServer>()
        .load::<UiDocumentAsset>(
            select_bevy_asset_path_for_canonical_ui_document_role(UiDocumentRole::SaveGame)
                .unwrap(),
        );
    let options_document = app
        .world()
        .resource::<AssetServer>()
        .load::<UiDocumentAsset>(
            select_bevy_asset_path_for_canonical_ui_document_role(UiDocumentRole::InGameOptions)
                .unwrap(),
        );
    for failure in [
        UiDocumentAssetLoadFailed {
            owner: failed_save_owner,
            document: save_document.clone(),
        },
        UiDocumentAssetLoadFailed {
            owner: unrelated_document_owner,
            document: options_document,
        },
        UiDocumentAssetLoadFailed {
            owner: foreign_owner,
            document: save_document,
        },
    ] {
        app.world_mut().write_message(failure);
    }
    app.update();
    assert!(app
        .world()
        .get::<OpenLoadSlotCatalogueAfterWorldSnapshotSave>(failed_save_owner)
        .is_none());
    for owner in [unrelated_document_owner, unaffected_owner] {
        assert!(app
            .world()
            .get::<OpenLoadSlotCatalogueAfterWorldSnapshotSave>(owner)
            .is_some());
    }
    assert_eq!(
        app.world_mut()
            .resource_mut::<Messages<ShowUiRole>>()
            .drain()
            .count(),
        0
    );

    app.world_mut().write_message(WorldSnapshotSavedToSlot {
        save_slot_identifier: SaveSlotId(4),
        saved_snapshot_byte_count: 100,
    });
    app.update();
    let opened = app
        .world_mut()
        .resource_mut::<Messages<ShowUiRole>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(opened.len(), 2);
    for owner in [unrelated_document_owner, unaffected_owner] {
        assert!(opened.contains(&ShowUiRole {
            role: UiDocumentRole::SavedGames,
            owner
        }));
    }
    assert!(!opened
        .iter()
        .any(|request| request.owner == failed_save_owner));
}
