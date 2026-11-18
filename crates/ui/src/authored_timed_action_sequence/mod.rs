use std::time::Duration;

use bevy::{prelude::*, time::Stopwatch};
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_interface_state::{
    input::input_types::ActionSource, ui::authored_ui_activation_contracts::UiNodeActivated,
};

#[derive(Component)]
#[relationship(relationship_target = UiTimedActions)]
pub(super) struct UiTimedActionOf(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = UiTimedActionOf, linked_spawn)]
pub(super) struct UiTimedActions(Vec<Entity>);

#[derive(Component)]
pub(super) struct UiTimedActionDue(pub(super) Duration);

#[derive(Component, Default)]
pub(super) struct UiAuthoredTimedActionSequencePlayback(Option<UiTimedActionClock>);

#[derive(Default)]
struct UiTimedActionClock {
    elapsed: Stopwatch,
    emitted: usize,
}

impl UiAuthoredTimedActionSequencePlayback {
    pub(super) fn set_active_and_restart(&mut self, active: bool) {
        self.0 = active.then(UiTimedActionClock::default);
    }
}

pub(super) fn advance_authored_timed_action_sequences_and_activate_due_proxies(
    time: Res<Time<Real>>,
    mut sequences: Query<(&UiTimedActions, &mut UiAuthoredTimedActionSequencePlayback)>,
    deadlines: Query<&UiTimedActionDue>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    for (actions, mut playback) in &mut sequences {
        if playback.0.is_none() {
            continue;
        }
        let Some(clock) = playback.0.as_mut() else {
            continue;
        };
        clock.elapsed.tick(time.delta());
        for node in actions.iter().skip(clock.emitted) {
            let Ok(deadline) = deadlines.get(node) else {
                break;
            };
            if deadline.0 > clock.elapsed.elapsed() {
                break;
            }
            activated.write(UiNodeActivated {
                source: ActionSource::System,
                node,
                trigger: UiTrigger::Press,
            });
            clock.emitted += 1;
        }
        if clock.emitted == actions.len() {
            playback.0 = None;
        }
    }
}
