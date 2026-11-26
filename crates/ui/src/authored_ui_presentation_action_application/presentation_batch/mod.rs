use crate::{
    authored_timed_action_sequence::{
        TimedUiPresentationCommitted, UiSelectionPresentationCommitted, UiTimedActionDue,
    },
    projection::ui_document_opening::UiDocumentOpeningEffects,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::ui_document::action::{presentation::UiPresentationAction, UiTrigger};
use openzt2_interface_state::{
    input::input_types::ActionSource,
    ui::{
        authored_ui_action_projection_components::UiPresentationActions,
        authored_ui_activation_contracts::UiNodeActivated,
        authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot},
        ui_document_lifecycle_contracts::UiDocumentOperation,
    },
};
use openzt2_world_state::simulation_time::simulation_control_types::UiDocumentPauseTransfer;
use openzt2_zoo_state::scenario::timed_scenario_ui_actions::{
    PendingScenarioUiSelection, ScenarioUiSelectionCompleted,
};

use super::RequestUiPresentationAction;

mod receivers;

#[derive(Clone, Copy)]
struct PresentationContext {
    document: Entity,
    lifecycle_owner: Entity,
    source: ActionSource,
    selection: Option<ScenarioUiSelectionCompleted>,
}

#[derive(SystemParam)]
pub(crate) struct AuthoredPresentationWrites<'w, 's> {
    receivers: ParamSet<
        'w,
        's,
        (
            receivers::DocumentWrites<'w, 's>,
            receivers::VisibilityWrites<'w, 's>,
            receivers::InteractionWrites<'w, 's>,
            receivers::ValuesWrites<'w, 's>,
            receivers::DrawingWrites<'w, 's>,
            receivers::TextWrites<'w, 's>,
            receivers::SelectionWrites<'w, 's>,
            (
                receivers::AudioWrites<'w, 's>,
                receivers::CachedMessageWrites<'w, 's>,
            ),
        ),
    >,
}

impl AuthoredPresentationWrites<'_, '_> {
    fn apply(
        &mut self,
        context: PresentationContext,
        action: UiPresentationAction,
        emitted: &mut Vec<UiNodeActivated>,
    ) {
        use UiPresentationAction::*;
        match action {
            ShowDocumentRole { .. }
            | HideDocumentRole { .. }
            | SetDocumentNodeVisible { .. }
            | HideOwningDocument
            | HideOwningDocumentAfterAlertAcknowledgement
            | HideOwningDocumentAfterConfirmationDismissal => {
                self.receivers.p0().apply(context, action, emitted)
            }
            SetTargetNodeVisible { .. }
            | HideTargetNodeUnlessHovered { .. }
            | WindTargetShowHideAnimationToCurrentDirectionBoundary { .. }
            | ShowOnlyNamedChildOfTargetNode { .. }
            | SetAllEmotePresentationsVisible { .. } => {
                self.receivers.p1().apply(context, action, emitted);
            }
            ActivateTargetNodeWithPress { .. }
            | SetTargetNodeInteractionEnabled { .. }
            | WatchTextEdit { .. }
            | SetTargetNodeExpanded { .. }
            | SetTargetNodeModal { .. }
            | SetTargetNodeTooltip { .. }
            | SetTargetNodeCursor { .. }
            | SetTargetNodeHoverPresentation { .. } => {
                self.receivers.p2().apply(context, action, emitted);
            }
            SetTargetNodeIntegerValue { .. }
            | ChangeTargetNodeSliderOrScrollPositionByDelta { .. } => {
                self.receivers.p3().apply(context, action);
            }
            SetTargetNodeImageSourceRectangle { .. }
            | SetTargetNodeLayoutPosition { .. }
            | SetTargetNodeLayoutSize { .. }
            | SetTargetNodeImageOverride { .. } => {
                self.receivers.p4().apply(context, action);
            }
            CopySourceNodeTextToTargetNode { .. }
            | FormatTargetNodeLocalizedInteger { .. }
            | SetTargetNodeTextFromLocalizationKey { .. }
            | SetTargetNodeTextLiteral { .. } => {
                self.receivers.p5().apply(context, action);
            }
            SetTargetNodeSelectionAndTimedSequencesActive { .. }
            | SelectTargetNode { .. }
            | ActivateSelectedChildOfTargetNode { .. }
            | FocusPreviousOrNextSiblingOfTargetNode { .. } => {
                self.receivers.p6().apply(context, action, emitted);
            }
            PlayDocumentAudioCue { .. } => self.receivers.p7().0.apply(context, action),
            AdvanceTargetNodeMessageCursor { .. } => self.receivers.p7().1.apply(context, action),
            MarkTargetNodeForQuery { .. } => {}
        }
    }
}

#[derive(SystemParam)]
pub(crate) struct AuthoredPresentationSources<'w, 's> {
    sources: Query<'w, 's, (&'static UiDocumentOwner, &'static UiPresentationActions)>,
    roots: Query<'w, 's, (&'static UiDocumentRoot, &'static ChildOf)>,
    documents: Res<'w, Assets<UiDocumentAsset>>,
}

enum BatchStep {
    Activate(UiNodeActivated, bool, Option<ScenarioUiSelectionCompleted>),
    Apply(PresentationContext, UiPresentationAction),
    ApplyOpenedDocumentEffect(Entity, UiDocumentOperation),
    FinishActivation,
}

pub(crate) fn apply_ordered_presentation_batches(
    mut commands: Commands,
    mut activations: ParamSet<(
        MessageReader<UiNodeActivated>,
        MessageWriter<UiNodeActivated>,
    )>,
    mut requests: MessageReader<RequestUiPresentationAction>,
    sources: AuthoredPresentationSources,
    mut writes: AuthoredPresentationWrites,
    mut ready: Query<
        (
            Entity,
            &mut UiDocumentOpeningEffects,
            &mut UiDocumentPauseTransfer,
        ),
        (
            With<UiDocumentRoot>,
            Or<(Changed<UiDocumentOpeningEffects>, Added<UiDocumentRoot>)>,
        ),
    >,
    timed_steps: Query<(), With<UiTimedActionDue>>,
    mut timed_committed: MessageWriter<TimedUiPresentationCommitted>,
    pending: Query<(
        &PendingScenarioUiSelection,
        Option<&mut UiSelectionPresentationCommitted>,
    )>,
) {
    let mut generated = Vec::new();
    let mut steps = Vec::new();
    for (document, mut effects, mut pause) in &mut ready {
        for effect in effects.drain() {
            steps.push(BatchStep::ApplyOpenedDocumentEffect(document, effect));
            run_batch(&mut steps, &sources, &mut writes, &mut generated);
        }
        pause.set_if_neq(UiDocumentPauseTransfer(false));
    }
    for activation in activations.p0().read() {
        steps.push(BatchStep::Activate(activation.clone(), false, None));
        run_batch(&mut steps, &sources, &mut writes, &mut generated);
        if timed_steps.contains(activation.node) {
            timed_committed.write(TimedUiPresentationCommitted(activation.node));
        }
    }
    let mut pending = pending;
    for request in requests.read() {
        let Ok((_, parent)) = sources.roots.get(request.document_root) else {
            continue;
        };
        let selection = match request.action {
            UiPresentationAction::SetTargetNodeSelectionAndTimedSequencesActive {
                target_node,
                ..
            } => pending
                .get(request.document_root)
                .ok()
                .filter(|(pending, _)| pending.0.target_node == target_node)
                .map(|(pending, _)| pending.0),
            _ => None,
        };
        steps.push(BatchStep::Apply(
            PresentationContext {
                document: request.document_root,
                lifecycle_owner: parent.parent(),
                source: ActionSource::System,
                selection,
            },
            request.action.clone(),
        ));
        run_batch(&mut steps, &sources, &mut writes, &mut generated);
        if let Some(selection) = selection {
            if let Ok((_, current)) = pending.get_mut(selection.document_root) {
                if let Some(mut current) = current {
                    current.set_if_neq(UiSelectionPresentationCommitted(selection));
                } else {
                    commands
                        .entity(selection.document_root)
                        .insert(UiSelectionPresentationCommitted(selection));
                }
            }
        }
    }
    let _ = activations.p1().write_batch(generated);
    // The action batch already consumed these descendants. Other systems
    // still receive their activations through the normal message stream.
    activations.p0().clear();
}

fn run_batch(
    steps: &mut Vec<BatchStep>,
    sources: &AuthoredPresentationSources,
    writes: &mut AuthoredPresentationWrites,
    generated: &mut Vec<UiNodeActivated>,
) {
    let mut active = Vec::<(Entity, UiTrigger)>::new();
    let mut cascading = Vec::new();
    while let Some(step) = steps.pop() {
        match step {
            BatchStep::Activate(activation, rebroadcast, selection) => {
                let key = (activation.node, activation.trigger);
                if active.contains(&key) {
                    continue;
                }
                let Ok((owner, actions)) = sources.sources.get(activation.node) else {
                    if rebroadcast {
                        generated.push(activation);
                    }
                    continue;
                };
                let document_root = owner.0;
                let Ok((root, parent)) = sources.roots.get(document_root) else {
                    continue;
                };
                let Some(document) = sources.documents.get(&root.document) else {
                    continue;
                };
                let context = PresentationContext {
                    document: document_root,
                    lifecycle_owner: parent.parent(),
                    source: activation.source,
                    selection,
                };
                active.push(key);
                steps.push(BatchStep::FinishActivation);
                let first_action = steps.len();
                steps.extend(
                    actions
                        .authored_action_records(document)
                        .filter(|record| record.trigger == activation.trigger)
                        .map(|record| BatchStep::Apply(context, record.action.clone())),
                );
                steps[first_action..].reverse();
                if rebroadcast {
                    generated.push(activation);
                }
            }
            BatchStep::Apply(context, action) => {
                writes.apply(context, action, &mut cascading);
                steps.extend(
                    cascading
                        .drain(..)
                        .rev()
                        .map(|activation| BatchStep::Activate(activation, true, context.selection)),
                );
            }
            BatchStep::ApplyOpenedDocumentEffect(document, effect) => {
                writes.receivers.p0().apply_opened_document_effect(
                    document,
                    effect,
                    &mut cascading,
                );
                steps.extend(
                    cascading
                        .drain(..)
                        .rev()
                        .map(|activation| BatchStep::Activate(activation, true, None)),
                );
            }
            BatchStep::FinishActivation => {
                active.pop();
            }
        }
    }
}

#[cfg(test)]
mod tests;
