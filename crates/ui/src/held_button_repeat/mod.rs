use std::time::Duration;

use bevy::{
    picking::{
        events::PointerState,
        pointer::{PointerButton, PointerId},
    },
    prelude::*,
};
use openzt2_game_data::ui_document::action::UiTrigger;

use openzt2_interface_state::input::input_types::ActionSource;

use super::authored_button_runtime_policy::UiButtonPolicy;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(Component, Default)]
pub(super) struct UiButtonHold(Option<Timer>);

pub(super) fn repeat_held_buttons(
    time: Res<Time>,
    pointer: Res<PointerState>,
    context: super::active_authored_ui_context::ActiveAuthoredUiContext,
    mut buttons: Query<(Entity, &UiButtonPolicy, &mut UiButtonHold)>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    let pressed = pointer.get(PointerId::Mouse, PointerButton::Primary);
    for (entity, policy, mut hold) in &mut buttons {
        let delay = Duration::try_from_secs_f32(policy.repeat_delay_seconds)
            .ok()
            .filter(|delay| !delay.is_zero());
        let active = pressed.is_some_and(|state| state.pressing.contains_key(&entity))
            && context.visible_and_enabled(entity);
        if !active || delay.is_none() {
            if hold.0.is_some() {
                hold.0 = None;
            }
            continue;
        }
        let Some(delay) = delay else { continue };
        let timer = hold
            .0
            .get_or_insert_with(|| Timer::new(delay, TimerMode::Once));
        let mut remaining = time.delta();
        let emit_repeat = |activated: &mut MessageWriter<UiNodeActivated>| {
            activated.write(UiNodeActivated {
                source: ActionSource::KeyboardMouse,
                node: entity,
                trigger: UiTrigger::Press,
            });
        };
        loop {
            let next_interval = (timer.duration().as_secs_f32() * policy.hold_change.max(0.0))
                .max(policy.hold_interval_cap_seconds);
            let next_interval = Duration::try_from_secs_f32(next_interval)
                .ok()
                .filter(|interval| !interval.is_zero());
            if next_interval == Some(timer.duration()) {
                timer.set_mode(TimerMode::Repeating);
                timer.tick(remaining);
                for _ in 0..timer.times_finished_this_tick() {
                    emit_repeat(&mut activated);
                }
                break;
            }
            timer.set_mode(TimerMode::Once);
            let until_repeat = timer.remaining();
            if remaining < until_repeat {
                timer.tick(remaining);
                break;
            }
            emit_repeat(&mut activated);
            let Some(next_interval) = next_interval else {
                hold.0 = None;
                break;
            };
            remaining -= until_repeat;
            timer.set_duration(next_interval);
            timer.reset();
        }
    }
}
