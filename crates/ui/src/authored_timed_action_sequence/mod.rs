use std::time::Duration;

use bevy::{prelude::*, time::Stopwatch};
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_interface_state::{
    input::input_types::ActionSource,
    ui::{
        authored_ui_activation_contracts::{RunAuthoredTimedScenarioScript, UiNodeActivated},
        authored_ui_node_projection_components::UiDocumentOwner,
    },
};

#[derive(Component)]
pub(super) struct TimedScenarioScript(pub(super) std::sync::Arc<str>);

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
    mut sequences: Query<(&Children, &mut UiAuthoredTimedActionSequencePlayback)>,
    deadlines: Query<(
        &UiTimedActionDue,
        &UiDocumentOwner,
        Option<&TimedScenarioScript>,
    )>,
    mut scripts: MessageWriter<RunAuthoredTimedScenarioScript>,
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
        let mut complete = true;
        let ordered = actions.iter().filter_map(|node| {
            let (deadline, owner, script) = deadlines.get(node).ok()?;
            Some((node, deadline, owner, script))
        });
        for (node, deadline, owner, script) in ordered.skip(clock.emitted) {
            if deadline.0 > clock.elapsed.elapsed() {
                complete = false;
                break;
            }
            if let Some(script) = script {
                scripts.write(RunAuthoredTimedScenarioScript {
                    document_root: owner.0,
                    command: script.0.clone(),
                });
            } else {
                activated.write(UiNodeActivated {
                    source: ActionSource::System,
                    node,
                    trigger: UiTrigger::Press,
                });
            }
            clock.emitted += 1;
        }
        if complete {
            playback.0 = None;
        }
    }
}
