//! Direct controller actions routed to existing gameplay message owners.

use super::input_types::ActionRequest;
use super::input_types::ActionSource;
use super::input_types::GameAction;
use bevy::prelude::*;
use openzt2_zoo_state::construction::construction_interaction_types::CancelConstruction;

pub(super) fn dispatch_controller_gameplay_actions(
    mut actions: MessageReader<ActionRequest>,
    mut cancel: MessageWriter<CancelConstruction>,
) {
    for request in actions
        .read()
        .filter(|request| matches!(request.source, ActionSource::Controller(_)))
    {
        if request.action == GameAction::SecondaryPointer {
            cancel.write(CancelConstruction);
        }
    }
}
