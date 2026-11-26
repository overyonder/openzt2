use std::time::Duration;

use bevy::{ecs::entity::EntityHashSet, prelude::*};
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_interface_state::{
    input::input_types::ActionSource,
    ui::{
        authored_ui_action_projection_components::UiScriptActions,
        authored_ui_activation_contracts::UiNodeActivated,
        authored_ui_node_projection_components::UiDocumentRoot,
    },
};
use openzt2_zoo_state::scenario::timed_scenario_ui_actions::{
    PendingScenarioUiSelection, RunAuthoredUiScript, ScenarioUiSelectionCompleted,
    TimedScenarioUiActionCompleted,
};

#[derive(Component)]
pub(super) struct UiTimedActionDue(pub(super) Duration);

#[derive(Component)]
pub(super) struct UiAuthoredTimedActionSequencePlayback {
    document: Entity,
    steps: Box<[Entity]>,
    starts_with_immediate_step: bool,
    clock: Option<UiTimedActionClock>,
}

#[derive(Default)]
struct UiTimedActionClock {
    elapsed_seconds: f32,
    next_step: usize,
    awaiting_script: Option<Entity>,
    awaiting_presentation: Option<Entity>,
    selection: Option<ScenarioUiSelectionCompleted>,
}

impl UiAuthoredTimedActionSequencePlayback {
    pub(super) fn new(
        document: Entity,
        steps: Box<[Entity]>,
        starts_with_immediate_step: bool,
        starts_active: bool,
    ) -> Self {
        Self {
            document,
            steps,
            starts_with_immediate_step,
            clock: starts_active.then(UiTimedActionClock::default),
        }
    }

    pub(super) fn restart(
        &mut self,
        active: bool,
        selection: Option<ScenarioUiSelectionCompleted>,
    ) -> Option<ScenarioUiSelectionCompleted> {
        let previous = self.clock.take().and_then(|clock| clock.selection);
        self.clock = active.then(|| UiTimedActionClock {
            selection: self
                .starts_with_immediate_step
                .then_some(selection)
                .flatten(),
            ..default()
        });
        previous
    }
}

#[derive(Message, Clone, Copy)]
pub(super) struct TimedUiPresentationCommitted(pub(super) Entity);

#[derive(Message, Clone, Copy)]
pub(super) struct UiSelectionWaitEnded(pub(super) ScenarioUiSelectionCompleted);

#[derive(Component, PartialEq, Eq)]
pub(super) struct UiSelectionPresentationCommitted(pub(super) ScenarioUiSelectionCompleted);

pub(super) fn release_selection_when_timed_sequence_disappears(
    removed: On<Remove<UiAuthoredTimedActionSequencePlayback>>,
    sequences: Query<&UiAuthoredTimedActionSequencePlayback>,
    mut released: MessageWriter<UiSelectionWaitEnded>,
) {
    if let Some(selection) = sequences
        .get(removed.entity)
        .ok()
        .and_then(|sequence| sequence.clock.as_ref())
        .and_then(|clock| clock.selection)
    {
        released.write(UiSelectionWaitEnded(selection));
    }
}

pub(super) fn dispatch_due_authored_sequence_steps(
    time: Res<Time<Real>>,
    mut sequences: Query<&mut UiAuthoredTimedActionSequencePlayback>,
    deadlines: Query<(&UiTimedActionDue, Option<&UiScriptActions>)>,
    documents: Query<&UiDocumentRoot>,
    assets: Option<Res<Assets<UiDocumentAsset>>>,
    mut scripts: Option<MessageWriter<RunAuthoredUiScript>>,
    mut completed_scripts: Option<MessageReader<TimedScenarioUiActionCompleted>>,
    mut committed_presentations: MessageReader<TimedUiPresentationCommitted>,
    mut activated: MessageWriter<UiNodeActivated>,
    mut ended: MessageWriter<UiSelectionWaitEnded>,
) {
    let committed: EntityHashSet = committed_presentations
        .read()
        .map(|commit| commit.0)
        .collect();
    let scripts_completed: EntityHashSet = completed_scripts
        .as_mut()
        .map(|completed| completed.read().map(|completion| completion.0).collect())
        .unwrap_or_default();
    for mut sequence in &mut sequences {
        if sequence.clock.is_none() {
            continue;
        }
        let UiAuthoredTimedActionSequencePlayback {
            document,
            steps,
            clock,
            ..
        } = &mut *sequence;
        let Some(active) = clock.as_mut() else {
            continue;
        };
        active.elapsed_seconds += time.delta_secs();
        if active
            .awaiting_presentation
            .is_some_and(|step| committed.contains(&step))
        {
            active.awaiting_presentation = None;
        }
        if active
            .awaiting_script
            .is_some_and(|step| scripts_completed.contains(&step))
        {
            active.awaiting_script = None;
        }
        if active.awaiting_script.is_none() {
            while let Some(&step) = steps.get(active.next_step) {
                let Ok((due, script)) = deadlines.get(step) else {
                    active.next_step += 1;
                    continue;
                };
                // Even a time-zero step requires strictly positive elapsed time.
                if active.elapsed_seconds <= due.0.as_secs_f32() {
                    break;
                }
                if script.is_some() && active.awaiting_presentation.is_some() {
                    break;
                }
                active.next_step += 1;
                if let Some(script) = script {
                    if let (Some(scripts), Some(asset)) = (
                        scripts.as_mut(),
                        documents
                            .get(*document)
                            .ok()
                            .and_then(|root| assets.as_ref()?.get(&root.document)),
                    ) {
                        if let Some((invocation, handle)) =
                            script.invocations(asset, UiTrigger::Press).next()
                        {
                            scripts.write(RunAuthoredUiScript {
                                action: step,
                                document_root: *document,
                                invocation: invocation.clone(),
                                handle: handle.cloned(),
                                timed: true,
                            });
                            active.awaiting_script = Some(step);
                            break;
                        }
                    }
                } else {
                    activated.write(UiNodeActivated {
                        source: ActionSource::System,
                        node: step,
                        trigger: UiTrigger::Press,
                    });
                    active.awaiting_presentation = Some(step);
                }
            }
        }
        let handoff = active.awaiting_script.is_some()
            || active.next_step == steps.len()
            || steps
                .get(active.next_step)
                .and_then(|step| deadlines.get(*step).ok())
                .is_some_and(|(due, _)| !due.0.is_zero());
        if handoff && active.awaiting_presentation.is_none() {
            if let Some(selection) = active.selection.take() {
                ended.write(UiSelectionWaitEnded(selection));
            }
        }
        if active.next_step == steps.len()
            && active.awaiting_script.is_none()
            && active.awaiting_presentation.is_none()
        {
            *clock = None;
        }
    }
}

pub(super) fn acknowledge_settled_scenario_ui_selections(
    mut commands: Commands,
    roots: Query<(
        Entity,
        Ref<UiSelectionPresentationCommitted>,
        &PendingScenarioUiSelection,
    )>,
    sequences: Query<&UiAuthoredTimedActionSequencePlayback>,
    mut released: MessageReader<UiSelectionWaitEnded>,
    mut completed: Option<MessageWriter<ScenarioUiSelectionCompleted>>,
) {
    let released: EntityHashSet = released.read().map(|event| event.0.document_root).collect();
    let Some(completed) = completed.as_mut() else {
        return;
    };
    for (root, committed, pending) in &roots {
        if !(committed.is_changed() || released.contains(&root)) || committed.0 != pending.0 {
            continue;
        }
        let waiting = sequences.iter().any(|sequence| {
            sequence
                .clock
                .as_ref()
                .is_some_and(|clock| clock.selection == Some(committed.0))
        });
        if !waiting {
            completed.write(committed.0);
            commands
                .entity(root)
                .remove::<(UiSelectionPresentationCommitted, PendingScenarioUiSelection)>();
        }
    }
}
