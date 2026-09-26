use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;

use crate::plugins::input::input_types::ActionSource;

use super::authored_button_runtime_policy::UiButtonPolicy;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct UiButtonHold {
    elapsed_seconds: f32,
    interval_seconds: f32,
}

pub(super) fn repeat_held_buttons(
    time: Res<Time>,
    mut buttons: Query<(Entity, &Interaction, &UiButtonPolicy, &mut UiButtonHold)>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    for (entity, interaction, policy, mut hold) in &mut buttons {
        if *interaction != Interaction::Pressed || policy.repeat_delay_seconds <= 0.0 {
            if hold.elapsed_seconds != 0.0 {
                hold.elapsed_seconds = 0.0;
            }
            if hold.interval_seconds != policy.repeat_delay_seconds {
                hold.interval_seconds = policy.repeat_delay_seconds;
            }
            continue;
        }
        if hold.interval_seconds <= 0.0 {
            hold.interval_seconds = policy.repeat_delay_seconds;
        }
        hold.elapsed_seconds += time.delta_secs();
        while hold.elapsed_seconds >= hold.interval_seconds {
            let remaining_seconds = hold.elapsed_seconds - hold.interval_seconds;
            // A zero or sub-precision interval cannot advance the repeat clock.
            if remaining_seconds >= hold.elapsed_seconds {
                hold.elapsed_seconds = 0.0;
                break;
            }
            hold.elapsed_seconds = remaining_seconds;
            activated.write(UiNodeActivated {
                source: ActionSource::KeyboardMouse,
                node: entity,
                trigger: UiTrigger::Press,
            });
            hold.interval_seconds = (hold.interval_seconds * policy.hold_change.max(0.0))
                .max(policy.hold_interval_cap_seconds);
        }
    }
}
