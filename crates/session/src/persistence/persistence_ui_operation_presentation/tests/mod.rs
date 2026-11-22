use bevy::prelude::*;
use openzt2_game_data::ui_document::document::UiDocumentRole;

use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;
use openzt2_interface_state::ui::ui_document_asset_load_failure::UiDocumentAssetLoadFailed;
use openzt2_interface_state::ui::ui_document_lifecycle_contracts::ShowUiRole;
use openzt2_interface_state::ui::UiSet;

use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;

use crate::persistence::persistence_ui_types::{
    OpenLoadSlotCatalogueAfterWorldSnapshotSave, SaveDialogSnapshotSaved, SaveDialogSubmission,
};
use crate::shell::shell_navigation_request_types::{ExitApplication, ReturnToMainMenu};
use openzt2_game_assets::ui_document::{
    ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    ui_document_role_asset_path_selection::select_bevy_asset_path_for_canonical_ui_document_role,
};

use super::continue_navigation_after_matching_dialog_save;

#[test]
fn failed_save_receiver_cannot_continue_after_a_later_save_for_other_owners() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<UiDocumentAsset>()
        .add_message::<UiNodeActivated>()
        .add_message::<UiDocumentAssetLoadFailed>()
        .add_message::<SaveDialogSnapshotSaved>()
        .add_message::<ReturnToMainMenu>()
        .add_message::<ExitApplication>()
        .add_message::<ShowUiRole>()
        .add_systems(
            Update,
            continue_navigation_after_matching_dialog_save.after(UiSet::Projection),
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
            document: save_document.clone(),
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

    let matching_dialog = app
        .world_mut()
        .spawn((
            UiDocumentRoot {
                document: save_document,
            },
            ChildOf(unaffected_owner),
        ))
        .id();
    let operation = app.world_mut().spawn_empty().id();
    app.world_mut().write_message(SaveDialogSnapshotSaved {
        operation,
        dialog: SaveDialogSubmission {
            document: matching_dialog,
            owner: unaffected_owner,
        },
    });
    app.update();
    let opened = app
        .world_mut()
        .resource_mut::<Messages<ShowUiRole>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(
        opened,
        [ShowUiRole {
            role: UiDocumentRole::SavedGames,
            owner: unaffected_owner,
        }]
    );
    assert!(app
        .world()
        .get::<OpenLoadSlotCatalogueAfterWorldSnapshotSave>(unrelated_document_owner)
        .is_some());
    assert!(!opened
        .iter()
        .any(|request| request.owner == failed_save_owner));
}
