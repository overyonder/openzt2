//! Profile and save-slot actions from UI documents.

use bevy::{prelude::*, text::EditableText};
use openzt2_game_data::ui_document::{
    action::persistence::UiPersistenceAction, document::UiDocumentRole,
};

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::ui::{
        authored_reusable_list_and_table_runtime_types::{UiListPolicy, UiListRow},
        authored_text_edit_bevy_adaptation::UiTextEditPolicy,
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_node_projection_components::UiDocumentRoot,
        authored_ui_node_projection_components::UiValue,
        authored_ui_selection_state::UiSelected,
        ui_document_lifecycle_contracts::ShowUiRole,
    },
};

use super::{
    persistence_failure_types::{
        WorldSnapshotPersistenceFailed, WorldSnapshotPersistenceFailure,
        WorldSnapshotPersistenceOperation,
    },
    persistence_ui_types::{
        OpenLoadSlotCatalogueAfterWorldSnapshotSave, PersistenceUiActionMessageWriters,
    },
    profile_types::{CreateProfile, DeleteProfile, ProfileIndex, SelectProfile},
    save_slot_types::{
        DeleteWorldSnapshotFromSlot, LoadSaveSlotCatalogue, LoadWorldSnapshotFromSlot, SaveSlotId,
        SaveWorldSnapshotToSlot,
    },
};
use crate::plugins::ui::authored_ui_action_projection_components::UiPersistenceActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

pub(super) fn route_authored_persistence_ui_actions_to_domain_requests(
    mut activated_ui_nodes: MessageReader<UiNodeActivated>,
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    persistence_action_nodes: Query<(&UiPersistenceActions, &UiDocumentOwner)>,
    ui_document_roots: Query<(&UiDocumentRoot, &ChildOf)>,
    selected_rows: Query<(&UiListRow, &UiSelected, &UiValue)>,
    lists: Query<(&UiDocumentOwner, &UiListPolicy)>,
    mut persistence_action_messages: PersistenceUiActionMessageWriters,
    mut commands: Commands,
    ui_values: Query<&UiValue>,
    editable_text_nodes: Query<(&EditableText, &UiDocumentOwner, Option<&UiTextEditPolicy>)>,
    profile_index: Res<ProfileIndex>,
    live_world_roots: Query<
        Entity,
        With<crate::plugins::world_spawn::world_membership_types::WorldRoot>,
    >,
) {
    for activated_ui_node in activated_ui_nodes.read() {
        let Ok((authored_persistence_action_range, ui_document_owner)) =
            persistence_action_nodes.get(activated_ui_node.node)
        else {
            continue;
        };
        let Ok((ui_document_root, lifecycle_owner)) = ui_document_roots.get(ui_document_owner.0)
        else {
            continue;
        };
        let Some(ui_document_asset) = ui_document_assets.get(&ui_document_root.document) else {
            continue;
        };
        let authored_persistence_actions =
            authored_persistence_action_range.authored_action_records(ui_document_asset);
        for authored_persistence_action in authored_persistence_actions {
            if activated_ui_node.trigger != authored_persistence_action.trigger {
                continue;
            }
            match &authored_persistence_action.action {
                UiPersistenceAction::SaveWorldSnapshotToSlot { save_slot } => {
                    persistence_action_messages
                        .save_world_snapshot_requests
                        .write(SaveWorldSnapshotToSlot {
                            save_slot_identifier: SaveSlotId(*save_slot),
                        });
                }
                UiPersistenceAction::LoadWorldSnapshotFromSlot { save_slot } => {
                    persistence_action_messages
                        .load_world_snapshot_requests
                        .write(LoadWorldSnapshotFromSlot {
                            save_slot_identifier: SaveSlotId(*save_slot),
                        });
                }
                UiPersistenceAction::SaveWorldSnapshotToSelectedSlot => {
                    let slot = ui_values
                        .get(activated_ui_node.node)
                        .map_or(0, |value| value.0.max(0) as u32);
                    let submitted_world_display_name =
                        editable_text_nodes
                            .iter()
                            .find_map(|(text, text_owner, policy)| {
                                (text_owner.0 == ui_document_owner.0
                                    && policy.is_some_and(|policy| {
                                        policy.accepts_only_legal_filename_characters()
                                    }))
                                .then(|| text.value().to_string())
                            });
                    if let Some(name) = submitted_world_display_name {
                        let name = name.trim();
                        if name.is_empty() || name.chars().any(char::is_control) {
                            persistence_action_messages
                                .world_snapshot_persistence_failures
                                .write(WorldSnapshotPersistenceFailed {
                                    persistence_operation:
                                        WorldSnapshotPersistenceOperation::SaveToSlot,
                                    save_slot_identifier: SaveSlotId(slot),
                                    failure_reason:
                                        WorldSnapshotPersistenceFailure::CorruptSnapshotSection,
                                });
                            continue;
                        }
                        if name.len()
                            > super::slot_io::world_snapshot_types::MAXIMUM_WORLD_DISPLAY_NAME_UTF8_BYTE_COUNT
                        {
                            persistence_action_messages.world_snapshot_persistence_failures.write(WorldSnapshotPersistenceFailed {
                                persistence_operation: WorldSnapshotPersistenceOperation::SaveToSlot,
                                save_slot_identifier: SaveSlotId(slot),
                                failure_reason: WorldSnapshotPersistenceFailure::CapacityExceeded,
                            });
                            continue;
                        }
                        if let Ok(ui_document_root) = live_world_roots.single() {
                            commands
                                .entity(ui_document_root)
                                .insert(Name::new(name.to_owned()));
                        }
                    }
                    persistence_action_messages
                        .save_world_snapshot_requests
                        .write(SaveWorldSnapshotToSlot {
                            save_slot_identifier: SaveSlotId(slot),
                        });
                }
                UiPersistenceAction::DeleteWorldSnapshotFromSelectedSlot => {
                    let slot = ui_values
                        .get(activated_ui_node.node)
                        .map_or(0, |value| value.0.max(0) as u32);
                    persistence_action_messages
                        .delete_world_snapshot_requests
                        .write(DeleteWorldSnapshotFromSlot {
                            save_slot_identifier: SaveSlotId(slot),
                        });
                }
                UiPersistenceAction::CreateProfileFromSubmittedDisplayName => {
                    let requested_profile_display_name = editable_text_nodes
                        .get(activated_ui_node.node)
                        .map_or_else(|_| String::new(), |(text, _, _)| text.value().to_string());
                    persistence_action_messages
                        .create_profile_requests
                        .write(CreateProfile {
                            requested_profile_display_name,
                        });
                }
                UiPersistenceAction::SelectProfileAtActivatedRow => {
                    let index = ui_values
                        .get(activated_ui_node.node)
                        .map_or(0, |value| value.0.max(0) as usize);
                    if let Some(profile) = profile_index.profile_records.get(index) {
                        persistence_action_messages
                            .select_profile_requests
                            .write(SelectProfile {
                                requested_profile_identifier: profile.profile_identifier,
                            });
                    }
                }
                UiPersistenceAction::DeleteProfileAtActivatedRow => {
                    let index = ui_values
                        .get(activated_ui_node.node)
                        .map_or(0, |value| value.0.max(0) as usize);
                    if let Some(profile) = profile_index.profile_records.get(index) {
                        persistence_action_messages
                            .delete_profile_requests
                            .write(DeleteProfile {
                                requested_profile_identifier: profile.profile_identifier,
                            });
                    }
                }
                UiPersistenceAction::LoadWorldSnapshotFromSelectedSlot => {
                    let Some(slot) =
                        super::save_slot_catalogue_ui_presentation::selected_save_slot_in_document(
                            ui_document_owner.0,
                            &selected_rows,
                            &lists,
                        )
                    else {
                        continue;
                    };
                    persistence_action_messages
                        .load_world_snapshot_requests
                        .write(LoadWorldSnapshotFromSlot {
                            save_slot_identifier: SaveSlotId(slot),
                        });
                }
                UiPersistenceAction::OpenSaveSlotCatalogueForSaving
                | UiPersistenceAction::OpenSaveSlotCatalogueForLoading => {
                    let role = if matches!(
                        authored_persistence_action.action,
                        UiPersistenceAction::OpenSaveSlotCatalogueForSaving
                    ) {
                        UiDocumentRole::SaveGame
                    } else {
                        UiDocumentRole::SavedGames
                    };
                    // Share the lifecycle owner so this document gets one canvas transform.
                    persistence_action_messages
                        .show_ui_document_requests
                        .write(ShowUiRole {
                            role,
                            owner: lifecycle_owner.parent(),
                        });
                    persistence_action_messages
                        .load_save_slot_catalogue_requests
                        .write(LoadSaveSlotCatalogue);
                }
                UiPersistenceAction::OpenLoadSlotCatalogueAfterSaveCompletes => {
                    commands
                        .entity(ui_document_owner.0)
                        .insert(OpenLoadSlotCatalogueAfterWorldSnapshotSave);
                }
            }
        }
    }
}
