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

use super::active_authored_ui_context::ActiveAuthoredUiContext;
use super::authored_hotkey_keyboard_activation::UiAuthoredHotkeyKeyboardActivationBinding;
use super::authored_ui_action_projection_components::UiShellActions;
use super::authored_ui_action_projection_components::{
    UiInformationActions, UiPresentationActions,
};
use super::authored_ui_activation_contracts::UiNodeActivated;
use super::authored_ui_node_projection_components::UiDocumentOwner;
use super::authored_ui_node_projection_components::UiDocumentRoot;
use super::authored_ui_node_projection_components::UiNodeId;
use super::authored_ui_target_lookup::AuthoredUiTargetLookup;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::ActionSource;
use openzt2_interface_state::input::input_types::GameAction;
use openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture;
use openzt2_interface_state::ui::input_frame_capture_types::OverheadModeInputCapture;

#[derive(SystemParam)]
pub(super) struct UiCancellationReceivers<'w, 's> {
    context: ActiveAuthoredUiContext<'w, 's>,
    capture: Res<'w, AuthoredModalInputCapture>,
    overhead_mode: Res<'w, OverheadModeInputCapture>,
    input_focus: Res<'w, InputFocus>,
    text_edits: Query<'w, 's, (), With<EditableText>>,
    documents: Res<'w, Assets<UiDocumentAsset>>,
    roots: Query<'w, 's, &'static UiDocumentRoot>,
    owners: Query<'w, 's, &'static UiDocumentOwner>,
    children: Query<'w, 's, &'static Children>,
    nodes: Query<
        'w,
        's,
        (Entity, &'static UiDocumentOwner, &'static UiNodeId),
        With<InheritedVisibility>,
    >,
    targets: AuthoredUiTargetLookup<'w, 's>,
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
    let modal = capture.0.or_else(|| context.top_modal());
    let edit = ui
        .input_focus
        .get()
        .filter(|node| ui.text_edits.contains(*node) && context.visible_and_enabled(*node));
    let active_scope = context.active_scope_with_modal(modal);
    let scope = modal
        .and_then(|entity| owners.get(entity).ok())
        .or_else(|| edit.and_then(|entity| owners.get(entity).ok()))
        .map(|owner| context.document_scope(owner.0))
        .or_else(|| {
            matches!(source, ActionSource::Controller(_))
                .then_some(active_scope)
                .flatten()
        });
    let candidates = ui
        .bindings
        .iter()
        .filter_map(|(_, parent, binding, _, shell)| {
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
            Some((receiver, owner, application_root))
        });
    let receiver = candidates.max_by_key(|(receiver, _, application_root)| {
        (!*application_root, context.navigation_order(*receiver))
    });
    if source == ActionSource::KeyboardMouse
        && receiver.is_none_or(|(_, owner, _)| Some(context.document_scope(owner)) != active_scope)
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
    if let Some((receiver, owner, _)) = receiver {
        activate_cancel_hotkey_receiver(&ui, receiver, owner, modal, source, &mut activated);
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
                    let mut found = false;
                    for action in &node.actions {
                        let (action_trigger, cancels) = cancellation_action(
                            action,
                            modal_id,
                            modal.is_none_or(|modal| modal == owner.0),
                        );
                        if action_trigger == *trigger {
                            if !cancels {
                                return false;
                            }
                            found = true;
                        }
                    }
                    found
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

fn activate_cancel_hotkey_receiver(
    ui: &UiCancellationReceivers,
    receiver: Entity,
    owner: Entity,
    modal: Option<Entity>,
    source: ActionSource,
    activated: &mut MessageWriter<UiNodeActivated>,
) {
    let Some(document) = ui
        .roots
        .get(owner)
        .ok()
        .and_then(|root| ui.documents.get(&root.document))
    else {
        return;
    };
    let scope = ui.context.document_scope(owner);
    let mut forwarded_targets = std::collections::HashSet::new();
    for (entity, parent, binding, presentation, shell) in ui
        .children
        .get(receiver)
        .into_iter()
        .flat_map(|children| children.iter())
        .filter_map(|child| ui.bindings.get(child).ok())
    {
        if parent.parent() != receiver
            || !binding.is_cancel_activation()
            || (!ui.overhead_mode.0 && binding_opens_overhead_options(ui, owner, shell))
        {
            continue;
        }
        let forwarded = presentation
            .and_then(|actions| actions.authored_action_records(document).next())
            .and_then(|record| match record.action {
                UiPresentationAction::ActivateTargetNodeWithPress { target_node } => {
                    Some(target_node)
                }
                _ => None,
            });
        let node = if let Some(target) = forwarded {
            ui.targets
                .candidates(owner, target)
                .find(|entity| ui.context.eligible_in_scope(*entity, scope, modal))
                .or_else(|| {
                    ui.targets.candidates(owner, target).find(|entity| {
                        ui.context.enabled(*entity)
                            && modal.is_none_or(|modal| ui.context.is_in_modal(*entity, modal))
                    })
                })
        } else {
            Some(entity)
        };
        if let Some(node) = node {
            if forwarded.is_none() || forwarded_targets.insert(node) {
                activated.write(UiNodeActivated {
                    node,
                    source,
                    trigger: UiTrigger::Press,
                });
            }
        }
    }
}

fn cancellation_action(
    record: &UiActionRecord,
    modal_id: Option<openzt2_game_data::AssetId>,
    can_hide_document: bool,
) -> (UiTrigger, bool) {
    match record {
        UiActionRecord::Presentation(record) => (
            record.trigger,
            (can_hide_document
                && matches!(
                    record.action,
                    UiPresentationAction::HideOwningDocument
                        | UiPresentationAction::HideOwningDocumentAfterAlertAcknowledgement
                        | UiPresentationAction::HideOwningDocumentAfterConfirmationDismissal
                ))
                || matches!(record.action, UiPresentationAction::SetTargetNodeVisible { target_node, visible: false }
                if Some(target_node) == modal_id),
        ),
        UiActionRecord::Information(record) => (
            record.trigger,
            matches!(
                record.action,
                UiInformationAction::ApplySetting {
                    setting: InformationSettingAction::Back
                }
            ),
        ),
        UiActionRecord::Shell(record) => (
            record.trigger,
            matches!(
                record.action,
                UiShellAction::DismissExitConfirmationDialog
                    | UiShellAction::NavigateBackFromDownloads
                    | UiShellAction::NavigateBackFromOptions
                    | UiShellAction::NavigateBackFromMapSelection
            ),
        ),
        UiActionRecord::Construction(record) => (
            record.trigger,
            matches!(
                record.action,
                UiConstructionAction::ResolveConfirmation {
                    confirmed: false,
                    ..
                }
            ),
        ),
        UiActionRecord::Transport(record) => (
            record.trigger,
            matches!(
                record.action,
                UiTransportAction::ResolvePendingTransportPathDeletion {
                    deletion_confirmed: false
                }
            ),
        ),
        UiActionRecord::Show(record) => (
            record.trigger,
            matches!(
                record.action,
                UiShowAction::ResolvePlatformDeletion { confirmed: false }
            ),
        ),
        UiActionRecord::Photo(record) => (
            record.trigger,
            matches!(record.action, UiPhotoAction::CancelMovingSelectedPhoto),
        ),
        UiActionRecord::PopulateCatalogueTypeList(record) => (record.trigger, false),
        UiActionRecord::Camera(record) => (record.trigger, false),
        UiActionRecord::StartResearchForSelectedCatalogueItem(record) => (record.trigger, false),
        UiActionRecord::AnimalHealth(record) => (record.trigger, false),
        UiActionRecord::Scenario(record) => (record.trigger, false),
        UiActionRecord::AudioSetting(record) => (record.trigger, false),
        UiActionRecord::EnterImmersiveMode(record) => (record.trigger, false),
        UiActionRecord::Staff(record) => (record.trigger, false),
        UiActionRecord::Economy(record) => (record.trigger, false),
        UiActionRecord::Animal(record) => (record.trigger, false),
        UiActionRecord::Persistence(record) => (record.trigger, false),
        UiActionRecord::Simulation(record) => (record.trigger, false),
    }
}
