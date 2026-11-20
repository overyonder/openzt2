use super::{
    construction_history::ConstructionHistory, construction_lifecycle::ConstructionActive,
};
use bevy::prelude::*;

use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::GameAction;

use openzt2_zoo_state::construction::construction_edit_history_types::ApplyConstructionTransaction;
use openzt2_zoo_state::construction::construction_edit_history_types::ConstructionEditDirection;

pub(super) fn no_pending_edits(pending: Query<(), With<ConstructionActive>>) -> bool {
    pending.is_empty()
}

pub(super) fn request_undo_redo(
    mut actions: MessageReader<ActionRequest>,
    history: ConstructionHistory,
    mut apply: MessageWriter<ApplyConstructionTransaction>,
) {
    for request in actions.read() {
        let direction = match request.action {
            GameAction::Undo => ConstructionEditDirection::Undo,
            GameAction::Redo => ConstructionEditDirection::Redo,
            _ => continue,
        };
        if let Some(transaction) = history.next(direction) {
            apply.write(ApplyConstructionTransaction {
                transaction,
                direction,
            });
            break;
        }
    }
}
