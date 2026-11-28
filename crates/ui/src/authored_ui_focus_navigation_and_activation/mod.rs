use bevy::prelude::*;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::ui_document::action::information::InformationSettingAction;
use openzt2_game_data::ui_document::action::information::UiInformationAction;
use openzt2_game_data::ui_document::action::UiTrigger;

use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::ActionSource;
use openzt2_interface_state::input::input_types::ActiveInputDevice;
use openzt2_interface_state::input::input_types::GameAction;

use super::{
    active_authored_ui_context::ActiveAuthoredUiContext,
    authored_button_runtime_policy::UiButtonPolicy,
    authored_hotkey_keyboard_activation::UiAuthoredHotkeyKeyboardActivationBinding,
    authored_text_edit_bevy_adaptation::UiTextEditPolicy,
    authored_ui_action_projection_components::UiInformationActions,
    authored_ui_focus_state::{UiFocusPresentation, UiFocusScope, UiFocusable},
    authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot},
    slider::{UiSliderAxis, UiSliderPolicy},
};
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiSubmitTarget;

#[expect(clippy::needless_pass_by_value)]
pub(super) fn navigate_authored_ui_focus_from_actions(
    mut actions: MessageReader<ActionRequest>,
    focusables: Query<(Entity, &UiFocusable, &InheritedVisibility, &UiDocumentOwner)>,
    sliders: Query<&UiSliderPolicy>,
    text_edits: Query<(), With<UiTextEditPolicy>>,
    mut scopes: Query<(Entity, &mut UiFocusScope)>,
    context: ActiveAuthoredUiContext,
) {
    let active_root = context.active_scope();
    let active_modal = context.top_modal();
    for request in actions.read() {
        let direction = match request.action {
            GameAction::NavigateUp | GameAction::NavigateLeft => -1,
            GameAction::NavigateDown | GameAction::NavigateRight => 1,
            _ => continue,
        };
        for (root, mut scope) in &mut scopes {
            if Some(root) != active_root {
                continue;
            }
            let handled_by_focused_control = scope
                .focused
                .filter(|focused| context.eligible_in_scope(*focused, root, active_modal))
                .is_some_and(|focused| {
                    (text_edits.contains(focused) && request.source == ActionSource::KeyboardMouse)
                        || sliders.get(focused).is_ok_and(|slider| {
                            matches!(
                                (slider.axis, request.action),
                                (
                                    UiSliderAxis::Horizontal | UiSliderAxis::Both,
                                    GameAction::NavigateLeft | GameAction::NavigateRight
                                ) | (
                                    UiSliderAxis::Vertical | UiSliderAxis::Both,
                                    GameAction::NavigateUp | GameAction::NavigateDown
                                )
                            )
                        })
                });
            if handled_by_focused_control {
                continue;
            }
            scope.focused = next_focusable_node_in_projected_document(
                root,
                scope.focused.or(scope.default),
                direction,
                &focusables,
                &context,
            );
        }
    }
}

fn next_focusable_node_in_projected_document(
    document_root: Entity,
    current: Option<Entity>,
    direction: i8,
    nodes: &Query<(Entity, &UiFocusable, &InheritedVisibility, &UiDocumentOwner)>,
    context: &ActiveAuthoredUiContext,
) -> Option<Entity> {
    let modal = context.top_modal();
    let current_order = current.and_then(|entity| {
        nodes
            .get(entity)
            .ok()
            .filter(|(entity, _, _, _)| context.eligible_in_scope(*entity, document_root, modal))
            .map(|(entity, focusable, _, _)| (focusable.order, context.navigation_order(entity)))
    });
    choose_wrapped_authored_ui_focus_candidate(
        current_order,
        direction,
        nodes.iter().filter_map(|(entity, focusable, _, _)| {
            context
                .eligible_in_scope(entity, document_root, modal)
                .then_some(((focusable.order, context.navigation_order(entity)), entity))
        }),
    )
}

pub(super) fn choose_wrapped_authored_ui_focus_candidate<T: Ord + Copy>(
    current_order: Option<T>,
    direction: i8,
    candidates: impl Iterator<Item = (T, Entity)>,
) -> Option<Entity> {
    let mut nearest: Option<(T, Entity)> = None;
    let mut wrap: Option<(T, Entity)> = None;
    for candidate in candidates {
        wrap = match (wrap, direction) {
            (None, _) => Some(candidate),
            (Some(found), 1) if candidate.0 < found.0 => Some(candidate),
            (Some(found), -1) if candidate.0 > found.0 => Some(candidate),
            (found, _) => found,
        };
        let is_after = match (current_order, direction) {
            (Some(order), 1) => candidate.0 > order,
            (Some(order), -1) => candidate.0 < order,
            (None, _) => true,
            _ => false,
        };
        if !is_after {
            continue;
        }
        nearest = match (nearest, direction) {
            (None, _) => Some(candidate),
            (Some(found), 1) if candidate.0 < found.0 => Some(candidate),
            (Some(found), -1) if candidate.0 > found.0 => Some(candidate),
            (found, _) => found,
        };
    }
    nearest.or(wrap).map(|(_, entity)| entity)
}

#[expect(clippy::needless_pass_by_value)]
pub(super) fn activate_focused_authored_ui_node_from_confirm_or_cancel(
    mut actions: MessageReader<ActionRequest>,
    scopes: Query<(Entity, &UiFocusScope)>,
    focusables: Query<(Entity, &UiFocusable, &InheritedVisibility, &UiDocumentOwner)>,
    context: ActiveAuthoredUiContext,
    cancel_bindings: Query<(&UiAuthoredHotkeyKeyboardActivationBinding, &UiDocumentOwner)>,
    nodes: Query<(
        &UiFocusable,
        &UiDocumentOwner,
        &InheritedVisibility,
        Option<&UiSubmitTarget>,
        Option<&UiButtonPolicy>,
    )>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    setting_actions: Query<(Entity, &UiDocumentOwner, &UiInformationActions)>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    let active_root = context.active_scope();
    let active_modal = context.top_modal();
    for request in actions.read() {
        if !matches!(request.action, GameAction::Confirm | GameAction::Cancel) {
            continue;
        }
        if request.action == GameAction::Cancel
            && matches!(request.source, ActionSource::Controller(_))
        {
            continue;
        }
        for (root, scope) in &scopes {
            if Some(root) != active_root {
                continue;
            }
            if request.action == GameAction::Cancel {
                let mut has_authored_cancel = false;
                for (binding, owner) in &cancel_bindings {
                    if context.document_scope(owner.0) != root
                        || !binding.is_cancel_activation()
                        || !context.hotkey_receiver_is_eligible(
                            owner.0,
                            binding.receiver_index(),
                            Some(root),
                            active_modal,
                        )
                    {
                        continue;
                    }
                    has_authored_cancel = true;
                    // Single-receiver cancellation activates this binding.
                }
                if has_authored_cancel {
                    continue;
                }
                // Settings has no authored Escape binding. Its semantic Back
                // action is the cancellation path; Apply also contains Back,
                // so never select an activation which commits settings first.
                if let Some((node, _, _)) = setting_actions.iter().find(|(node, owner, actions)| {
                    if !context.eligible_in_scope(*node, root, active_modal) {
                        return false;
                    }
                    let Some(document) = roots
                        .get(owner.0)
                        .ok()
                        .and_then(|root| documents.get(&root.document))
                    else {
                        return false;
                    };
                    let back = actions.authored_action_records(document).any(|record| {
                        record.trigger == UiTrigger::Press
                            && matches!(
                                record.action,
                                UiInformationAction::ApplySetting {
                                    setting: InformationSettingAction::Back
                                }
                            )
                    });
                    let accepts = actions.authored_action_records(document).any(|record| {
                        record.trigger == UiTrigger::Press
                            && matches!(
                                record.action,
                                UiInformationAction::ApplySetting {
                                    setting: InformationSettingAction::Accept
                                }
                            )
                    });
                    back && !accepts
                }) {
                    activated.write(UiNodeActivated {
                        node,
                        source: request.source,
                        trigger: UiTrigger::Press,
                    });
                    continue;
                }
            }
            let Some(node) = scope
                .focused
                .or(scope.default)
                .filter(|node| {
                    nodes
                        .get(*node)
                        .is_ok_and(|(focusable, _owner, visible, _, _)| {
                            context.eligible_in_scope(*node, root, active_modal)
                                && focusable.enabled
                                && visible.get()
                        })
                })
                .or_else(|| {
                    next_focusable_node_in_projected_document(root, None, 1, &focusables, &context)
                })
            else {
                continue;
            };
            let Ok((focusable, _owner, visible, submit, button)) = nodes.get(node) else {
                continue;
            };
            if focusable.enabled && visible.get() {
                activated.write(UiNodeActivated {
                    node,
                    source: request.source,
                    trigger: match request.action {
                        GameAction::Cancel => UiTrigger::Cancel,
                        GameAction::Confirm if submit.is_some() => UiTrigger::Submit,
                        GameAction::Confirm => UiTrigger::Press,
                        _ => unreachable!(),
                    },
                });
                if request.action == GameAction::Confirm
                    && button.is_some_and(|policy| !policy.selectable)
                {
                    activated.write(UiNodeActivated {
                        node,
                        source: request.source,
                        trigger: UiTrigger::On,
                    });
                    activated.write(UiNodeActivated {
                        node,
                        source: request.source,
                        trigger: UiTrigger::Off,
                    });
                }
            }
        }
    }
}

#[derive(Component)]
pub(super) struct AuthoredHoverPresentation;

#[expect(clippy::needless_pass_by_value)]
pub(super) fn present_controller_authored_ui_focus(
    active_input: Res<ActiveInputDevice>,
    context: ActiveAuthoredUiContext,
    scopes: Query<(Entity, Ref<UiFocusScope>)>,
    mut nodes: Query<(&UiDocumentOwner, &mut UiFocusPresentation), With<UiFocusable>>,
    authored_hover: Query<Entity, With<AuthoredHoverPresentation>>,
    mut highlighted: Local<Vec<Entity>>,
) {
    if !active_input.is_changed() && !scopes.iter().any(|(_, scope)| scope.is_changed()) {
        return;
    }
    let controller_focus = matches!(active_input.source, ActionSource::Controller(_));
    for entity in highlighted.drain(..).chain(authored_hover.iter()) {
        if let Ok((owner, mut focused)) = nodes.get_mut(entity) {
            let scope = scopes.get(context.document_scope(owner.0));
            if focused.0
                && scope.is_ok_and(|(_, scope)| !controller_focus || scope.focused != Some(entity))
            {
                focused.0 = false;
            }
        }
    }
    if controller_focus {
        for (root, scope) in &scopes {
            let Some(entity) = scope.focused else {
                continue;
            };
            let Ok((owner, mut focused)) = nodes.get_mut(entity) else {
                continue;
            };
            if context.document_scope(owner.0) != root {
                continue;
            }
            focused.set_if_neq(UiFocusPresentation(true));
            highlighted.push(entity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_order_is_deterministic_and_wraps() {
        let a = Entity::from_bits(1);
        let b = Entity::from_bits(2);
        let c = Entity::from_bits(3);
        let candidates = || [(30, c), (10, a), (20, b)].into_iter();
        assert_eq!(
            choose_wrapped_authored_ui_focus_candidate(Some(10), 1, candidates()),
            Some(b)
        );
        assert_eq!(
            choose_wrapped_authored_ui_focus_candidate(Some(30), 1, candidates()),
            Some(a)
        );
        assert_eq!(
            choose_wrapped_authored_ui_focus_candidate(Some(10), -1, candidates()),
            Some(c)
        );
    }
}
