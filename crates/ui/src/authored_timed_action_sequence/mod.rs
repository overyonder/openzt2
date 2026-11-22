use std::time::Duration;

use bevy::{prelude::*, time::Stopwatch};
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_interface_state::{
    input::input_types::ActionSource,
    ui::{
        authored_ui_activation_contracts::UiNodeActivated,
        authored_ui_node_projection_components::UiDocumentOwner,
    },
};
use openzt2_zoo_state::scenario::timed_scenario_ui_actions::{
    PendingScenarioUiSelection, RunTimedScenarioUiAction, ScenarioUiSelectionCompleted,
    TimedScenarioUiActionCompleted,
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
    emitted: bevy::platform::collections::HashSet<Entity>,
    awaiting_script: Option<Entity>,
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
    mut scripts: Option<MessageWriter<RunTimedScenarioUiAction>>,
    mut completed_scripts: Option<MessageReader<TimedScenarioUiActionCompleted>>,
    mut activations: ParamSet<(
        MessageReader<UiNodeActivated>,
        MessageWriter<UiNodeActivated>,
    )>,
    mut commands: Commands,
    selections: Query<(Entity, &PendingScenarioUiSelection)>,
    mut completed_selections: Option<MessageWriter<ScenarioUiSelectionCompleted>>,
) {
    // The presentation router runs before this system. Its deferred actions
    // emit further activations for the next pass through the authored chain.
    let authored_chain_active = activations.p0().read().fold(false, |active, event| {
        active || event.source == ActionSource::System
    });
    if !authored_chain_active {
        if let Some(completed) = completed_selections.as_mut() {
            for (document, selection) in &selections {
                completed.write(selection.0);
                commands
                    .entity(document)
                    .remove::<PendingScenarioUiSelection>();
            }
        }
    }
    let mut activated = activations.p1();
    let completed = completed_scripts
        .as_mut()
        .map(|messages| {
            messages
                .read()
                .map(|message| message.0)
                .collect::<bevy::platform::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    for (actions, mut playback) in &mut sequences {
        if playback.0.is_none() {
            continue;
        }
        let Some(clock) = playback.0.as_mut() else {
            continue;
        };
        // Authored deadlines use single-precision seconds and fire strictly
        // after the deadline, including entries authored at time zero.
        let seconds = clock.elapsed.elapsed_secs() + time.delta_secs();
        clock.elapsed.tick(time.delta());
        if clock.elapsed.elapsed_secs() != seconds {
            clock.elapsed.set_elapsed(Duration::from_secs_f32(seconds));
        }
        if let Some(action) = clock.awaiting_script {
            if !completed.contains(&action) {
                continue;
            }
            clock.awaiting_script = None;
        }
        let mut complete = true;
        let ordered = actions.iter().filter_map(|node| {
            let (deadline, owner, script) = deadlines.get(node).ok()?;
            Some((node, deadline, owner, script))
        });
        for (node, deadline, owner, script) in ordered {
            if clock.emitted.contains(&node) {
                continue;
            }
            if deadline.0.as_secs_f32() >= seconds {
                complete = false;
                continue;
            }
            clock.emitted.insert(node);
            if let Some(script) = script {
                if let Some(writer) = scripts.as_mut() {
                    writer.write(RunTimedScenarioUiAction {
                        action: node,
                        document_root: owner.0,
                        command: script.0.clone(),
                    });
                    clock.awaiting_script = Some(node);
                    complete = false;
                    break;
                }
            } else {
                activated.write(UiNodeActivated {
                    source: ActionSource::System,
                    node,
                    trigger: UiTrigger::Press,
                });
            }
        }
        if complete {
            playback.0 = None;
        }
    }
}
