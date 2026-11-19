use bevy::{
    ecs::entity::EntityHashSet,
    input_focus::{FocusCause, InputFocus},
    picking::{
        events::{
            PointerCancel, PointerClick, PointerDragEnd, PointerOut, PointerOver, PointerPress,
            PointerState,
        },
        hover::HoverMap,
        pointer::{PointerButton, PointerId},
    },
    prelude::*,
};
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_interface_state::{
    input::input_types::ActiveInputDevice,
    ui::{
        authored_ui_activation_contracts::UiNodeActivated,
        input_frame_capture_types::AuthoredModalInputCapture,
    },
};

use super::{
    active_authored_ui_context::ActiveAuthoredUiContext,
    authored_button_runtime_policy::UiButtonPolicy, authored_ui_focus_state::UiFocusable,
};

#[derive(Component)]
pub struct LastAuthoredPrimaryPress(f64);

impl Default for LastAuthoredPrimaryPress {
    fn default() -> Self {
        Self(f64::NEG_INFINITY)
    }
}

pub(super) fn activate_authored_pointer_events(
    time: Res<Time<Real>>,
    input: Res<ActiveInputDevice>,
    context: ActiveAuthoredUiContext,
    modal: Res<AuthoredModalInputCapture>,
    pointer: Res<PointerState>,
    hover: Res<HoverMap>,
    mut overs: MessageReader<PointerOver>,
    mut outs: MessageReader<PointerOut>,
    mut presses: MessageReader<PointerPress>,
    mut clicks: MessageReader<PointerClick>,
    mut drag_ends: MessageReader<PointerDragEnd>,
    mut cancellations: MessageReader<PointerCancel>,
    mut controls: Query<(
        Has<UiFocusable>,
        Option<&UiButtonPolicy>,
        &mut LastAuthoredPrimaryPress,
    )>,
    mut focus: ResMut<InputFocus>,
    mut output: MessageWriter<UiNodeActivated>,
) {
    let enabled = |entity| {
        context.visible_and_enabled(entity)
            && modal
                .0
                .is_none_or(|modal| context.is_in_modal(entity, modal))
    };
    let pressed = pointer.get(PointerId::Mouse, PointerButton::Primary);
    let mut emit = |node, trigger| {
        output.write(UiNodeActivated {
            node,
            source: input.source,
            trigger,
        });
    };
    let mut departures = EntityHashSet::default();
    for event in outs.read().filter(|event| event.pointer.id.is_mouse()) {
        if controls.contains(event.entity)
            && !pressed.is_some_and(|state| state.pressing.contains_key(&event.entity))
        {
            departures.insert(event.entity);
            emit(event.entity, UiTrigger::Leave);
        }
    }
    for event in overs.read().filter(|event| event.pointer.id.is_mouse()) {
        if controls.contains(event.entity) && enabled(event.entity) {
            emit(event.entity, UiTrigger::Enter);
        }
    }
    for event in presses
        .read()
        .filter(|event| event.pointer.id.is_mouse() && event.button == PointerButton::Primary)
    {
        let Ok((focusable, button, mut last_press)) = controls.get_mut(event.entity) else {
            continue;
        };
        if !enabled(event.entity) {
            continue;
        }
        if focusable && focus.get() != Some(event.entity) {
            focus.set(event.entity, FocusCause::Pressed);
        }
        emit(event.entity, UiTrigger::Press);
        let now = time.elapsed_secs_f64();
        if now - last_press.0 <= 0.5 {
            emit(event.entity, UiTrigger::Submit);
        }
        last_press.0 = now;
        if button.is_some_and(|policy| !policy.selectable) {
            emit(event.entity, UiTrigger::On);
        }
    }
    let cancelled = cancellations
        .read()
        .filter(|event| event.pointer.id.is_mouse())
        .map(|event| event.entity);
    let dragged = drag_ends
        .read()
        .filter(|event| event.pointer.id.is_mouse() && event.button == PointerButton::Primary)
        .map(|event| event.entity);
    let clicked = clicks
        .read()
        .filter(|event| event.pointer.id.is_mouse() && event.button == PointerButton::Primary)
        .map(|event| event.entity);
    let hovered = hover.get(&PointerId::Mouse);
    let mut released = EntityHashSet::default();
    for entity in clicked.chain(dragged).chain(cancelled) {
        let Ok((_, button, _)) = controls.get(entity) else {
            continue;
        };
        if !released.insert(entity) {
            continue;
        }
        if !hovered.is_some_and(|hits| hits.contains_key(&entity)) && departures.insert(entity) {
            emit(entity, UiTrigger::Leave);
        }
        if button.is_some_and(|policy| !policy.selectable) && enabled(entity) {
            emit(entity, UiTrigger::Off);
        }
    }
}
