use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    action::{
        immersive_mode::UiImmersiveModeKind, information::UiInformationAction,
        presentation::UiPresentationAction,
    },
    document::UiDocumentRole,
};

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::plugins::persistence::save_slot_types::WorldSnapshotLoadedFromSlot;

use super::authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot};
use super::authored_ui_selection_binding::UiSelectedBinding;
use super::authored_ui_selection_state::UiSelected;
use crate::plugins::ui::authored_ui_action_projection_components::{
    UiImmersiveModeActions, UiInformationActions, UiPresentationActions,
};

/// Restores the initial toggle presentation established by the original
/// `setup_main_gui_toolbar` after UI document projects the authored HUD and after a
/// successful saved-world load refreshes the existing in-game controls.
///
/// The authored document remains the sole owner of the controls, their order,
/// layout, help text, visuals, and domain actions. Matching the typed action
/// records avoids retaining source widget names or numeric control identities.
pub(super) fn initialize_main_toolbar(
    documents: Res<Assets<UiDocumentAsset>>,
    mut completed_loads: MessageReader<WorldSnapshotLoadedFromSlot>,
    roots: Query<(Entity, Ref<UiDocumentRoot>)>,
    mut toggles: Query<(
        &UiDocumentOwner,
        &mut UiSelected,
        Option<&UiInformationActions>,
        Option<&UiImmersiveModeActions>,
        Option<&UiPresentationActions>,
        Has<UiSelectedBinding>,
    )>,
) {
    let world_loaded = completed_loads.read().next().is_some();
    for (root_entity, root) in &roots {
        if !root.is_added() && !world_loaded {
            continue;
        }
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let data = document.canonical_ui_document();
        if !matches!(&data.role, UiDocumentRole::InGameHud) {
            continue;
        }

        for (owner, mut selected, information, immersive_modes, presentation, bound) in &mut toggles
        {
            if owner.0 != root_entity || bound {
                continue;
            }

            let initial = information
                .filter(|actions| {
                    actions.authored_action_records(document).any(|record| {
                        matches!(&record.action, UiInformationAction::SetViewFilter { .. })
                    })
                })
                .map(|_| false)
                .or_else(|| {
                    immersive_modes
                        .filter(|actions| {
                            actions
                                .authored_action_records(document)
                                .any(|record| record.mode == UiImmersiveModeKind::GuestView)
                        })
                        .map(|_| false)
                })
                .or_else(|| {
                    presentation
                        .filter(|actions| {
                            actions.authored_action_records(document).any(|record| {
                                matches!(
                                    &record.action,
                                    UiPresentationAction::SetAllEmotePresentationsVisible { .. }
                                )
                            })
                        })
                        .map(|_| false)
                });

            if let Some(initial) = initial.filter(|initial| selected.0 != *initial) {
                selected.0 = initial;
            }
        }
    }
}
