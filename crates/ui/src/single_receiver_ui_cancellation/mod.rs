//! One cancel request dismisses one authored activity, for keyboard Escape and
//! controller cancel alike, without requiring a focused cancel button.

use bevy::{ecs::system::SystemParam, input_focus::InputFocus, prelude::*, text::EditableText};
use openzt2_game_data::ui_document::action::animal_shows::UiShowAction;
use openzt2_game_data::ui_document::action::construction::UiConstructionAction;
use openzt2_game_data::ui_document::action::information::InformationSettingAction;
use openzt2_game_data::ui_document::action::information::UiInformationAction;
use openzt2_game_data::ui_document::action::photography::UiPhotoAction;
use openzt2_game_data::ui_document::action::presentation::UiPresentationAction;
use openzt2_game_data::ui_document::action::shell_navigation::UiShellAction;
use openzt2_game_data::ui_document::action::transportation::UiTransportAction;
use openzt2_game_data::ui_document::action::UiActionRecord;
use openzt2_game_data::ui_document::action::UiTrigger;

use super::active_authored_ui_context::{ActiveAuthoredUiContext, AuthoredFocusScope};
use super::authored_hotkey_keyboard_activation::UiAuthoredHotkeyKeyboardActivationBinding;
use super::authored_ui_action_projection_components::UiShellActions;
use super::authored_ui_action_projection_components::{
    UiInformationActions, UiPresentationActions,
};
use super::authored_ui_activation_contracts::UiNodeActivated;
use super::authored_ui_node_projection_components::UiDocumentOwner;
use super::authored_ui_node_projection_components::UiDocumentRoot;
use super::authored_ui_node_projection_components::UiNodeId;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::ActionSource;
use openzt2_interface_state::input::input_types::GameAction;
use openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture;
use openzt2_interface_state::ui::input_frame_capture_types::OverheadModeInputCapture;

#[derive(SystemParam)]
pub(super) struct UiCancellationReceivers<'w, 's> {
    context: ActiveAuthoredUiContext<'w, 's>,
    scope: Res<'w, AuthoredFocusScope>,
    capture: Res<'w, AuthoredModalInputCapture>,
    overhead_mode: Res<'w, OverheadModeInputCapture>,
    input_focus: Res<'w, InputFocus>,
    text_edits: Query<'w, 's, (), With<EditableText>>,
    documents: Res<'w, Assets<UiDocumentAsset>>,
    roots: Query<'w, 's, &'static UiDocumentRoot>,
    owners: Query<'w, 's, &'static UiDocumentOwner>,
    nodes: Query<
        'w,
        's,
        (Entity, &'static UiDocumentOwner, &'static UiNodeId),
        With<InheritedVisibility>,
    >,
    settings: Query<
        'w,
        's,
        (
            Entity,
            &'static UiDocumentOwner,
            &'static UiInformationActions,
        ),
    >,
    bindings: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static UiAuthoredHotkeyKeyboardActivationBinding,
            Option<&'static UiPresentationActions>,
            Option<&'static UiShellActions>,
        ),
    >,
}

pub(super) fn dismiss_one_active_ui_activity_from_cancel_requests(
    mut requests: MessageReader<ActionRequest>,
    ui: UiCancellationReceivers,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    let UiCancellationReceivers {
        context,
        capture,
        documents,
        roots,
        owners,
        nodes,
        ..
    } = &ui;
    // Multiple cancel buttons or devices in one input frame still dismiss only
    // one activity. Drain all requests so none can replay against its parent.
    let source = requests
        .read()
        .filter_map(|request| {
            (request.action == GameAction::Cancel
                && matches!(
                    request.source,
                    ActionSource::Controller(_) | ActionSource::KeyboardMouse
                ))
            .then_some(request.source)
        })
        .last();
    let Some(source) = source else {
        return;
    };
    let modal = capture.0.or(ui.scope.modal);
    let active_scope = context.active_scope_with_modal(modal);
    let edit = ui
        .input_focus
        .get()
        .filter(|node| ui.text_edits.contains(*node) && context.visible_and_enabled(*node));
    let scope = modal
        .and_then(|entity| owners.get(entity).ok())
        .or_else(|| edit.and_then(|entity| owners.get(entity).ok()))
        .map(|owner| context.document_scope(owner.0))
        .or_else(|| {
            (matches!(source, ActionSource::Controller(_)) && modal == ui.scope.modal)
                .then_some(ui.scope.document)
                .flatten()
        });
    let candidates = ui
        .bindings
        .iter()
        .filter_map(|(node, parent, binding, _, shell)| {
            let receiver = parent.parent();
            let owner = owners.get(receiver).ok()?.0;
            if !binding.is_cancel_activation()
                || !context.visible_and_enabled(receiver)
                || modal.is_some_and(|modal| !context.is_in_modal(receiver, modal))
                || scope.is_some_and(|scope| context.document_scope(owner) != scope)
                || (!ui.overhead_mode.0 && binding_opens_overhead_options(&ui, owner, shell))
            {
                return None;
            }
            let application_root = roots
                .get(owner)
                .ok()
                .and_then(|root| documents.get(&root.document))
                .is_some_and(|document| {
                    document.canonical_ui_document().role
                        == openzt2_game_data::ui_document::document::UiDocumentRole::ApplicationRoot
                });
            Some((node, receiver, owner, application_root))
        });
    let receiver = candidates.max_by_key(|(node, receiver, _, application_root)| {
        (
            !*application_root,
            context.navigation_order(*receiver),
            std::cmp::Reverse(node.to_bits()),
        )
    });
    if source == ActionSource::KeyboardMouse
        && receiver
            .is_none_or(|(_, _, owner, _)| Some(context.document_scope(owner)) != active_scope)
    {
        if let Some(scope) = active_scope {
            if let Some((node, _, _)) = ui.settings.iter().find(|(node, owner, actions)| {
                if !context.eligible_in_scope(*node, scope, modal) {
                    return false;
                }
                let Some(document) = roots
                    .get(owner.0)
                    .ok()
                    .and_then(|root| documents.get(&root.document))
                else {
                    return false;
                };
                let press = || {
                    actions
                        .authored_action_records(document)
                        .filter(|record| record.trigger == UiTrigger::Press)
                };
                press().any(|record| {
                    matches!(
                        record.action,
                        UiInformationAction::ApplySetting {
                            setting: InformationSettingAction::Back
                        }
                    )
                }) && !press().any(|record| {
                    matches!(
                        record.action,
                        UiInformationAction::ApplySetting {
                            setting: InformationSettingAction::Accept
                        }
                    )
                })
            }) {
                activated.write(UiNodeActivated {
                    node,
                    source,
                    trigger: UiTrigger::Press,
                });
                return;
            }
        }
    }
    if let Some((node, ..)) = receiver {
        activated.write(UiNodeActivated {
            node,
            source,
            trigger: UiTrigger::Press,
        });
        return;
    }
    if source == ActionSource::KeyboardMouse {
        if let Some(node) = ui.input_focus.get().filter(|node| {
            active_scope.is_some_and(|scope| context.eligible_in_scope(*node, scope, modal))
        }) {
            activated.write(UiNodeActivated {
                node,
                source,
                trigger: UiTrigger::Cancel,
            });
        }
        return;
    }
    let Some(scope) = scope.or(active_scope) else {
        return;
    };
    let modal_id = modal
        .and_then(|modal| nodes.get(modal).ok())
        .map(|(_, _, id)| id.id);
    let target = nodes
        .iter()
        .filter_map(|(entity, owner, id)| {
            if !context.eligible_in_scope(entity, scope, modal) {
                return None;
            }
            let document = documents.get(&roots.get(owner.0).ok()?.document)?;
            let node = document
                .canonical_ui_document()
                .nodes
                .get(id.index as usize)?;
            // Reject mixed actions such as Apply + Back, or Save + Hide. Only the
            // cancellation trigger's records matter; hover/focus actions are inert.
            [UiTrigger::Cancel, UiTrigger::Press]
                .into_iter()
                .find(|trigger| {
                    let mut actions = node
                        .actions
                        .iter()
                        .filter(|action| action.trigger() == *trigger)
                        .peekable();
                    actions.peek().is_some()
                        && actions.all(|action| {
                            dismisses_active_activity(
                                action,
                                modal_id,
                                modal.is_none_or(|modal| modal == owner.0),
                            )
                        })
                })
                .map(|trigger| (entity, trigger))
        })
        .max_by_key(|(entity, _)| context.navigation_order(*entity));
    if let Some((node, trigger)) = target {
        activated.write(UiNodeActivated {
            node,
            source,
            trigger,
        });
    }
}

fn dismisses_active_activity(
    action: &UiActionRecord,
    modal_id: Option<openzt2_game_data::AssetId>,
    owns_modal: bool,
) -> bool {
    match action {
        UiActionRecord::Presentation(record) => match record.action {
            UiPresentationAction::HideOwningDocument
            | UiPresentationAction::HideOwningDocumentAfterAlertAcknowledgement
            | UiPresentationAction::HideOwningDocumentAfterConfirmationDismissal => owns_modal,
            UiPresentationAction::SetTargetNodeVisible {
                target_node,
                visible: false,
            } => Some(target_node) == modal_id,
            _ => false,
        },
        UiActionRecord::Information(record) => matches!(
            record.action,
            UiInformationAction::ApplySetting {
                setting: InformationSettingAction::Back
            }
        ),
        UiActionRecord::Shell(record) => matches!(
            record.action,
            UiShellAction::DismissExitConfirmationDialog
                | UiShellAction::NavigateBackFromDownloads
                | UiShellAction::NavigateBackFromOptions
                | UiShellAction::NavigateBackFromMapSelection
        ),
        UiActionRecord::Construction(record) => matches!(
            record.action,
            UiConstructionAction::ResolveConfirmation {
                confirmed: false,
                ..
            }
        ),
        UiActionRecord::Transport(record) => matches!(
            record.action,
            UiTransportAction::ResolvePendingTransportPathDeletion {
                deletion_confirmed: false
            }
        ),
        UiActionRecord::Show(record) => matches!(
            record.action,
            UiShowAction::ResolvePlatformDeletion { confirmed: false }
        ),
        UiActionRecord::Photo(record) => {
            matches!(record.action, UiPhotoAction::CancelMovingSelectedPhoto)
        }
        _ => false,
    }
}

fn binding_opens_overhead_options(
    ui: &UiCancellationReceivers,
    owner: Entity,
    shell: Option<&UiShellActions>,
) -> bool {
    let Some(document) = ui
        .roots
        .get(owner)
        .ok()
        .and_then(|root| ui.documents.get(&root.document))
    else {
        return false;
    };
    shell.is_some_and(|actions| {
        actions
            .authored_action_records(document)
            .any(|record| record.action == UiShellAction::OpenInGameOptionsFromOverheadEscape)
    })
}
