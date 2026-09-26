use bevy::{prelude::*, window::PrimaryWindow};

use crate::plugins::{
    economy::money_types::Money, world_spawn::world_membership_types::WorldMember,
};

use super::construction_edit_history_types::ConstructionEditHistory;
use super::construction_interaction_types::{
    CommitConstruction, ConstructionPreview, PlacementValidity,
};
use super::construction_transaction_types::{
    ConstructionFeedbackOrigin, EditCommitPhase, EditCommitProgress, EditTransaction,
    PrepareConstruction, TransactionState,
};

/// Opens one construction transaction only from the exact still-live valid
/// preview. The confirming pointer coordinate is retained solely for feedback
/// after the domain and economy transaction has fully committed.
pub(crate) fn begin_construction_transaction_from_valid_preview(
    mut commands: Commands,
    mut construction_commit_requests: MessageReader<CommitConstruction>,
    previews: Query<(&ConstructionPreview, &WorldMember)>,
    pending_transactions: Query<&EditCommitProgress>,
    mut construction_edit_history: ResMut<ConstructionEditHistory>,
    mut construction_preparation_requests: MessageWriter<PrepareConstruction>,
    primary_windows: Query<&Window, With<PrimaryWindow>>,
) {
    if pending_transactions
        .iter()
        .any(|progress| progress.phase != EditCommitPhase::Complete)
    {
        return;
    }
    for construction_commit_request in construction_commit_requests.read() {
        let Ok((preview, world_member)) = previews.get(construction_commit_request.preview) else {
            continue;
        };
        if !matches!(preview.validity, PlacementValidity::Valid { .. }) {
            continue;
        }
        let Some(transaction_sequence) = construction_edit_history.allocate_sequence() else {
            continue;
        };
        let mut transaction = commands.spawn((
            EditTransaction {
                sequence: transaction_sequence,
                state: TransactionState::Applied,
                cost: Money(0),
            },
            EditCommitProgress::preparing(),
            *world_member,
        ));
        if let Some(confirming_pointer_screen_position) = primary_windows
            .single()
            .ok()
            .and_then(Window::cursor_position)
        {
            transaction.insert(ConstructionFeedbackOrigin(
                confirming_pointer_screen_position,
            ));
        }
        let transaction = transaction.id();
        construction_preparation_requests.write(PrepareConstruction {
            transaction,
            preview: construction_commit_request.preview,
        });
        break;
    }
}
