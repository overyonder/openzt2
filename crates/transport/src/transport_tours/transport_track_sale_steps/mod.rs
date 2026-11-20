use bevy::{ecs::system::SystemParam, prelude::*};

use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::{
    ApplyPreparedTransportTrackConstructionEditRequest,
    TransportTrackConstructionEditApplicationAcknowledged, TransportTrackConstructionEditSteps,
};

#[derive(Component)]
pub(super) struct TransportTrackSaleStep {
    pub(super) ordinal: usize,
    pub(super) state: TrackSaleStepState,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TrackSaleStepState {
    Queued,
    Applying,
    Applied,
    Reversing,
    Failed,
}

#[derive(Message)]
pub(super) struct ApplyTransportTrackSaleStep(
    pub(super) ApplyPreparedTransportTrackConstructionEditRequest,
);

#[derive(Message)]
pub(super) struct TransportTrackSaleStepAcknowledged(
    TransportTrackConstructionEditApplicationAcknowledged,
);

#[derive(SystemParam)]
pub(super) struct TrackApplicationAcknowledgements<'w, 's> {
    step_children: Query<'w, 's, (), (With<ChildOf>, With<TransportTrackSaleStep>)>,
    steps: MessageWriter<'w, TransportTrackSaleStepAcknowledged>,
    edits: MessageWriter<'w, TransportTrackConstructionEditApplicationAcknowledged>,
}

impl TrackApplicationAcknowledgements<'_, '_> {
    pub(super) fn write(&mut self, result: TransportTrackConstructionEditApplicationAcknowledged) {
        if self.step_children.contains(result.transaction) {
            self.steps.write(TransportTrackSaleStepAcknowledged(result));
        } else {
            self.edits.write(result);
        }
    }
}

fn dispatch_next_track_sale_step(
    transaction: &TransportTrackConstructionEditSteps,
    children: &Children,
    steps: &mut Query<&mut TransportTrackSaleStep>,
    requests: &mut MessageWriter<ApplyTransportTrackSaleStep>,
) -> bool {
    let Some(application) = transaction.application else {
        return false;
    };
    let reversing = application == EditApplication::FailureRollback;
    let (child_application, step_state, reverse_order) = if reversing {
        let undoing_undo = transaction.last_application == Some(EditApplication::Undo);
        (
            if undoing_undo {
                EditApplication::Redo
            } else {
                EditApplication::Undo
            },
            TrackSaleStepState::Applied,
            !undoing_undo,
        )
    } else {
        (
            application,
            TrackSaleStepState::Queued,
            application == EditApplication::Undo,
        )
    };
    let next = children
        .iter()
        .filter_map(|child| {
            let step = steps.get(child).ok()?;
            (step.state == step_state).then_some((
                if reverse_order {
                    usize::MAX - step.ordinal
                } else {
                    step.ordinal
                },
                child,
            ))
        })
        .min()
        .map(|(_, child)| child);
    let Some(child) = next else {
        return false;
    };
    let Ok(mut step) = steps.get_mut(child) else {
        return false;
    };
    step.state = if reversing {
        TrackSaleStepState::Reversing
    } else {
        TrackSaleStepState::Applying
    };
    requests.write(ApplyTransportTrackSaleStep(
        ApplyPreparedTransportTrackConstructionEditRequest {
            transaction: child,
            application: child_application,
        },
    ));
    true
}

pub(super) fn begin_requested_transport_track_sales(
    mut requests: MessageReader<ApplyPreparedTransportTrackConstructionEditRequest>,
    mut sales: Query<(&mut TransportTrackConstructionEditSteps, &Children)>,
    mut steps: Query<&mut TransportTrackSaleStep>,
    mut child_requests: MessageWriter<ApplyTransportTrackSaleStep>,
    mut results: MessageWriter<TransportTrackConstructionEditApplicationAcknowledged>,
) {
    for request in requests.read() {
        let Ok((mut sale, children)) = sales.get_mut(request.transaction) else {
            continue;
        };
        if sale.application.is_some() {
            continue;
        }
        sale.application = Some(request.application);
        if request.application != EditApplication::FailureRollback {
            sale.last_application = Some(request.application);
            for child in children.iter() {
                if let Ok(mut step) = steps.get_mut(child) {
                    step.state = TrackSaleStepState::Queued;
                }
            }
        }
        if !dispatch_next_track_sale_step(&sale, children, &mut steps, &mut child_requests) {
            sale.application = None;
            results.write(TransportTrackConstructionEditApplicationAcknowledged {
                transaction: request.transaction,
                application: request.application,
                accepted: true,
            });
        }
    }
}

pub(super) fn settle_acknowledged_transport_track_sale_steps(
    mut results: MessageReader<TransportTrackSaleStepAcknowledged>,
    parents: Query<&ChildOf, With<TransportTrackSaleStep>>,
    mut sales: Query<(&mut TransportTrackConstructionEditSteps, &Children)>,
    mut steps: Query<&mut TransportTrackSaleStep>,
    mut child_requests: MessageWriter<ApplyTransportTrackSaleStep>,
    mut completed: MessageWriter<TransportTrackConstructionEditApplicationAcknowledged>,
) {
    for &TransportTrackSaleStepAcknowledged(result) in results.read() {
        let Ok(parent) = parents.get(result.transaction) else {
            continue;
        };
        let Ok((mut sale, children)) = sales.get_mut(parent.parent()) else {
            continue;
        };
        let Some(application) = sale.application else {
            continue;
        };
        let Ok(mut step) = steps.get_mut(result.transaction) else {
            continue;
        };
        let reversing = application == EditApplication::FailureRollback;
        let expected = if reversing {
            TrackSaleStepState::Reversing
        } else {
            TrackSaleStepState::Applying
        };
        if step.state != expected {
            continue;
        }
        step.state = if !result.accepted {
            TrackSaleStepState::Failed
        } else if reversing {
            TrackSaleStepState::Queued
        } else {
            TrackSaleStepState::Applied
        };
        if !result.accepted && !reversing {
            sale.rejected_application = Some(application);
            sale.application = Some(EditApplication::FailureRollback);
        }
        if (result.accepted || !reversing)
            && dispatch_next_track_sale_step(&sale, children, &mut steps, &mut child_requests)
        {
            continue;
        }
        sale.application = None;
        let rejected_application = sale.rejected_application.take();
        completed.write(TransportTrackConstructionEditApplicationAcknowledged {
            transaction: parent.parent(),
            application: rejected_application.unwrap_or(application),
            accepted: result.accepted && rejected_application.is_none(),
        });
    }
}
