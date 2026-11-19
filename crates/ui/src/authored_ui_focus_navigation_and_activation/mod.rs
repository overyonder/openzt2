use bevy::{
    input_focus::{
        tab_navigation::{NavAction, TabGroup, TabNavigation},
        FocusCause, FocusGained, FocusLost, InputFocus, InputFocusVisible,
    },
    picking::pointer::{PointerButton, PointerInput},
    prelude::*,
};
use openzt2_game_data::ui_document::action::UiTrigger;

use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::ActionSource;
use openzt2_interface_state::input::input_types::ActiveInputDevice;
use openzt2_interface_state::input::input_types::GameAction;

use super::{
    active_authored_ui_context::ActiveAuthoredUiContext,
    authored_button_runtime_policy::UiButtonPolicy,
    authored_text_edit_bevy_adaptation::UiTextEditPolicy,
    authored_ui_focus_state::UiFocusable,
    authored_ui_node_projection_components::UiDocumentOwner,
    slider::{UiSliderAxis, UiSliderPolicy},
};
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiSubmitTarget;

#[derive(Component, Default)]
pub(super) struct UiFocusMemory {
    pub(super) remembered: Option<Entity>,
    pub(super) default: Option<Entity>,
}

#[derive(Component, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct AuthoredHoverPresentation(pub(super) bool);

pub(super) fn update_active_authored_focus(
    context: ActiveAuthoredUiContext,
    memory: Query<&UiFocusMemory>,
    controls: Query<(), With<UiFocusable>>,
    input: Res<ActiveInputDevice>,
    mut visibility: ResMut<InputFocusVisible>,
    mut focus: ResMut<InputFocus>,
) {
    let highlight = matches!(input.source, ActionSource::Controller(_));
    if visibility.0 != highlight {
        visibility.0 = highlight;
    }
    let current = focus.get();
    let modal = context.top_modal();
    let scope = context.active_scope();
    if current.is_some_and(|node| {
        !controls.contains(node)
            || scope.is_some_and(|scope| context.eligible_in_scope(node, scope, modal))
    }) {
        return;
    }
    let next = scope.and_then(|scope| {
        memory
            .get(scope)
            .ok()?
            .remembered
            .filter(|next| context.eligible_in_scope(*next, scope, modal))
    });
    if let Some(next) = next {
        focus.set(next, FocusCause::Navigated);
    } else if current.is_some() {
        focus.clear();
    }
}

pub(super) fn release_edit_focus_when_pressing_elsewhere(
    mut input: MessageReader<PointerInput>,
    capture: Res<openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture>,
    context: ActiveAuthoredUiContext,
    edits: Query<&UiDocumentOwner, With<bevy::text::EditableText>>,
    mut memory: Query<&mut UiFocusMemory>,
    mut focus: ResMut<InputFocus>,
) {
    let pressed = input.read().any(|event| {
        event.pointer_id.is_mouse() && event.button_just_pressed(PointerButton::Primary)
    });
    if !pressed {
        return;
    }
    let Some(current) = focus.get() else { return };
    let Ok(owner) = edits.get(current) else {
        return;
    };
    if capture.target == Some(current) {
        return;
    }
    if let Ok(mut scope) = memory.get_mut(context.document_scope(owner.0)) {
        if scope.remembered == Some(current) {
            scope.remembered = None;
        }
    }
    focus.clear();
}

pub(super) fn authored_focus_entered(
    event: On<FocusGained>,
    controls: Query<&UiDocumentOwner, With<UiFocusable>>,
    context: ActiveAuthoredUiContext,
    mut memory: Query<&mut UiFocusMemory>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    let node = event.event_target();
    if let Ok(owner) = controls.get(node) {
        if let Ok(mut memory) = memory.get_mut(context.document_scope(owner.0)) {
            memory.remembered = Some(node);
        }
        activated.write(UiNodeActivated {
            node,
            source: ActionSource::System,
            trigger: UiTrigger::Focus,
        });
    }
}

pub(super) fn authored_focus_left(
    event: On<FocusLost>,
    controls: Query<(), With<UiFocusable>>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    let node = event.event_target();
    if controls.contains(node) {
        activated.write(UiNodeActivated {
            node,
            source: ActionSource::System,
            trigger: UiTrigger::Blur,
        });
    }
}

pub(super) fn navigate_authored_ui_focus_from_actions(
    mut actions: MessageReader<ActionRequest>,
    focusables: Query<(Entity, &UiFocusable, &InheritedVisibility, &UiDocumentOwner)>,
    sliders: Query<&UiSliderPolicy>,
    text_edits: Query<(), With<UiTextEditPolicy>>,
    memory: Query<&UiFocusMemory>,
    mut focus: ResMut<InputFocus>,
    navigation: TabNavigation,
    groups: Query<&TabGroup>,
    context: ActiveAuthoredUiContext,
) {
    let active = context.active_scope();
    let modal = context.top_modal();
    for request in actions.read() {
        let direction = match request.action {
            GameAction::NavigateUp | GameAction::NavigateLeft => -1,
            GameAction::NavigateDown | GameAction::NavigateRight => 1,
            _ => continue,
        };
        let Some(root) = active else { continue };
        let current = focus
            .get()
            .filter(|node| context.eligible_in_scope(*node, root, modal));
        let handled = current.is_some_and(|focused| {
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
        if handled {
            continue;
        }
        let remembered = memory
            .get(root)
            .ok()
            .and_then(|memory| memory.remembered.or(memory.default));
        let from = current.or(remembered);
        let action = if direction == 1 {
            NavAction::Next
        } else {
            NavAction::Previous
        };
        let group = groups.get(root).ok().map(|group| (root, group));
        let next = navigation
            .navigate_internal(from, action, group)
            .ok()
            .filter(|node| context.eligible_in_scope(*node, root, modal))
            .or_else(|| {
                next_focusable_node_in_projected_document(
                    root,
                    from,
                    direction,
                    &focusables,
                    &context,
                )
            });
        if let Some(next) = next {
            if focus.get() != Some(next) {
                focus.set(next, FocusCause::Navigated);
            }
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

pub(super) fn activate_focused_authored_ui_node_from_confirm(
    mut actions: MessageReader<ActionRequest>,
    memory: Query<&UiFocusMemory>,
    focus: Res<InputFocus>,
    focusables: Query<(Entity, &UiFocusable, &InheritedVisibility, &UiDocumentOwner)>,
    context: ActiveAuthoredUiContext,
    nodes: Query<(
        &UiFocusable,
        &UiDocumentOwner,
        &InheritedVisibility,
        Option<&UiSubmitTarget>,
        Option<&UiButtonPolicy>,
    )>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    let active_modal = context.top_modal();
    let active_root = context.active_scope_with_modal(active_modal);
    for request in actions.read() {
        if request.action != GameAction::Confirm {
            continue;
        }
        let Some(root) = active_root else { continue };
        {
            let Some(node) = focus
                .get()
                .filter(|node| context.eligible_in_scope(*node, root, active_modal))
                .or_else(|| {
                    memory
                        .get(root)
                        .ok()
                        .and_then(|memory| memory.remembered.or(memory.default))
                })
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
                    trigger: if submit.is_some() {
                        UiTrigger::Submit
                    } else {
                        UiTrigger::Press
                    },
                });
                if button.is_some_and(|policy| !policy.selectable) {
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
