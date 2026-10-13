use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::economy::money_types::Money;

use openzt2_zoo_state::construction::construction_edit_history_types::ConstructionEditHistory;
use openzt2_zoo_state::construction::construction_interaction_types::CommitConstruction;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionFeedbackOrigin;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitPhase;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitProgress;
use openzt2_zoo_state::construction::construction_transaction_types::EditTransaction;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareConstruction;
use openzt2_zoo_state::construction::construction_transaction_types::TransactionState;

/// Opens one construction transaction only from the exact still-live valid
/// preview. The confirming pointer coordinate is retained solely for feedback
/// after the domain and economy transaction has fully committed.
pub fn begin_construction_transaction_from_valid_preview(
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
