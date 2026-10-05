//! One cancel request dismisses one authored activity, for keyboard Escape and
//! controller cancel alike, independently of focus.

use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::ui_document::action::{
    animal_shows::UiShowAction,
    construction::UiConstructionAction,
    information::{InformationSettingAction, UiInformationAction},
    photography::UiPhotoAction,
    presentation::UiPresentationAction,
    shell_navigation::UiShellAction,
    transportation::UiTransportAction,
    UiActionRecord, UiTrigger,
};

use super::{
    active_authored_ui_context::{
        ActiveAuthoredUiContext, AuthoredModalInputCapture, OverheadModeInputCapture,
    },
    authored_hotkey_keyboard_activation::UiAuthoredHotkeyKeyboardActivationBinding,
    authored_ui_action_projection_components::{UiPresentationActions, UiShellActions},
    authored_ui_activation_contracts::UiNodeActivated,
    authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot, UiNodeId},
};
use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::input::input_types::{ActionRequest, ActionSource, GameAction},
};

#[derive(SystemParam)]
pub(super) struct UiCancellationReceivers<'w, 's> {
    context: ActiveAuthoredUiContext<'w, 's>,
    capture: Res<'w, AuthoredModalInputCapture>,
    overhead_mode: Res<'w, OverheadModeInputCapture>,
    documents: Res<'w, Assets<UiDocumentAsset>>,
    roots: Query<'w, 's, &'static UiDocumentRoot>,
    owners: Query<'w, 's, &'static UiDocumentOwner>,
    nodes: Query<
        'w,
        's,
        (Entity, &'static UiDocumentOwner, &'static UiNodeId),
        With<InheritedVisibility>,
    >,
    bindings: Query<
        'w,
        's,
        (
            Entity,
            &'static UiDocumentOwner,
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
    let scope = match modal {
        Some(entity) => owners
            .get(entity)
            .ok()
            .map(|owner| context.document_scope(owner.0)),
        // Without a modal or focus, the topmost panel with an Escape binding
        // is dismissed; the zoo view's own binding sits beneath every panel.
        None => context
            .active_scope()
            .or_else(|| topmost_cancel_hotkey_receiver_scope(&ui)),
    };
    let Some(scope) = scope else {
        return;
    };

    if activate_cancel_hotkey_receiver(&ui, scope, modal, source, &mut activated) {
        return;
    }
    // The keyboard follows authored Escape bindings only. Controllers have no
    // Escape key, so they may also infer a dialog's cancel button.
    if source == ActionSource::KeyboardMouse {
        return;
    }

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

/// Whether this receiver holds an Escape binding that may act now. The zoo
/// view's own Escape (`ZT_ESCAPE_KEY`) exists only in overhead mode.
fn receiver_holds_active_cancel_binding(
    ui: &UiCancellationReceivers,
    owner: Entity,
    index: u32,
) -> bool {
    ui.bindings
        .iter()
        .any(|(_, binding_owner, binding, _, shell)| {
            binding_owner.0 == owner
                && binding.receiver_index() == index
                && binding.is_cancel_activation()
                && (ui.overhead_mode.0 || !binding_opens_overhead_options(ui, owner, shell))
        })
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

fn topmost_cancel_hotkey_receiver_scope(ui: &UiCancellationReceivers) -> Option<Entity> {
    ui.nodes
        .iter()
        .filter(|(entity, owner, id)| {
            ui.context.visible_and_enabled(*entity)
                && receiver_holds_active_cancel_binding(ui, owner.0, id.index)
        })
        .max_by_key(|(entity, _, _)| ui.context.navigation_order(*entity))
        .map(|(_, owner, _)| ui.context.document_scope(owner.0))
}

fn activate_cancel_hotkey_receiver(
    ui: &UiCancellationReceivers,
    scope: Entity,
    modal: Option<Entity>,
    source: ActionSource,
    activated: &mut MessageWriter<UiNodeActivated>,
) -> bool {
    let UiCancellationReceivers {
        context,
        documents,
        roots,
        nodes,
        bindings,
        ..
    } = ui;
    // A hotkey may lower to several action proxies. Keep that receiver's
    // complete action group, but never activate other Escape receivers too.
    let receiver = nodes
        .iter()
        .filter(|(entity, owner, id)| {
            context.eligible_in_scope(*entity, scope, modal)
                && receiver_holds_active_cancel_binding(ui, owner.0, id.index)
        })
        .max_by_key(|(entity, _, _)| context.navigation_order(*entity));
    let Some((_, owner, id)) = receiver else {
        return false;
    };
    let mut forwarded_targets = std::collections::HashSet::new();
    for (entity, binding_owner, binding, presentation, _) in bindings {
        if binding_owner.0 != owner.0
            || binding.receiver_index() != id.index
            || !binding.is_cancel_activation()
        {
            continue;
        }
        let forwarded_target = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
            .and_then(|document| {
                presentation.and_then(|actions| actions.authored_action_records(document).next())
            })
            .and_then(|record| match record.action {
                UiPresentationAction::ActivateTargetNodeWithPress { target_node } => {
                    Some(target_node)
                }
                _ => None,
            });
        let target = if let Some(target_id) = forwarded_target {
            let named = |_: Entity, target_owner: &UiDocumentOwner, id: &UiNodeId| {
                target_owner.0 == owner.0 && id.id == target_id
            };
            nodes
                .iter()
                .find_map(|(target, target_owner, id)| {
                    (named(target, target_owner, id)
                        && context.eligible_in_scope(target, scope, modal))
                    .then_some(target)
                })
                // Text buttons such as the in-game options Cancel stay hidden
                // until hovered. Escape presses them if they are enabled.
                .or_else(|| {
                    nodes.iter().find_map(|(target, target_owner, id)| {
                        (named(target, target_owner, id)
                            && context.enabled(target)
                            && modal.is_none_or(|modal| context.is_in_modal(target, modal)))
                        .then_some(target)
                    })
                })
        } else {
            Some(entity)
        };
        if let Some(node) = target {
            if forwarded_target.is_some() && !forwarded_targets.insert(node) {
                continue;
            }
            activated.write(UiNodeActivated {
                node,
                source,
                trigger: UiTrigger::Press,
            });
        }
    }
    true
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
