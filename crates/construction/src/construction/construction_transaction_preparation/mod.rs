use super::{
    construction_history::ConstructionHistoryPolicy,
    construction_lifecycle::{ConstructionActive, Preparing},
};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::economy::general_economy_types::GeneralEconomyTransaction;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::terrain::terrain_brush_types::{TerrainBrushKind, TerrainBrushPreview};

use openzt2_zoo_state::construction::construction_interaction_types::CommitConstruction;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionFeedbackOrigin;
use openzt2_zoo_state::construction::construction_transaction_types::EditTransaction;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareConstruction;
use openzt2_zoo_state::construction::construction_transaction_types::TransactionState;

#[derive(Component)]
pub(crate) struct SubmittedPreview<T: Component + Clone>(pub T);

pub(crate) fn capture_confirmed_preview<T: Component + Clone>(
    mut confirmations: MessageReader<CommitConstruction>,
    previews: Query<&T>,
    mut commands: Commands,
) {
    for confirmation in confirmations.read() {
        if let Ok(preview) = previews.get(confirmation.preview) {
            commands
                .entity(confirmation.preview)
                .try_insert_if_new(SubmittedPreview(preview.clone()));
        }
    }
}

pub(crate) fn clear_confirmed_previews<T: Component + Clone>(
    previews: Query<Entity, With<SubmittedPreview<T>>>,
    mut commands: Commands,
) {
    for preview in &previews {
        commands.entity(preview).try_remove::<SubmittedPreview<T>>();
    }
}

/// Opens one construction transaction only from the exact still-live valid
/// preview. The confirming pointer coordinate is retained solely for feedback
/// after the domain and economy transaction has fully committed.
pub(super) fn begin_construction_transaction_from_valid_preview(
    mut commands: Commands,
    mut construction_commit_requests: MessageReader<CommitConstruction>,
    previews: Query<(
        &ConstructionPreview,
        &WorldMember,
        Option<&TerrainBrushPreview>,
        Option<&SubmittedPreview<ConstructionPreview>>,
    )>,
    submitted: Query<Entity, With<SubmittedPreview<ConstructionPreview>>>,
    pending_transactions: Query<(), With<ConstructionActive>>,
    mut construction_edit_history: ResMut<ConstructionHistoryPolicy>,
    mut construction_preparation_requests: MessageWriter<PrepareConstruction>,
    primary_windows: Query<&Window, With<PrimaryWindow>>,
) {
    if !pending_transactions.is_empty() {
        construction_commit_requests.clear();
        return;
    }
    let requests = construction_commit_requests
        .read()
        .map(|request| request.preview)
        .collect::<Vec<_>>();
    for preview_entity in requests.into_iter().chain(&submitted) {
        let Ok((preview, world_member, terrain_brush, submission)) = previews.get(preview_entity)
        else {
            continue;
        };
        let preview = submission.map_or(preview, |submitted| &submitted.0);
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
            Preparing,
            *world_member,
            match terrain_brush.map(|brush| brush.kind) {
                None => GeneralEconomyTransaction::Build,
                Some(TerrainBrushKind::Paint { .. }) => GeneralEconomyTransaction::BiomeChange,
                Some(
                    TerrainBrushKind::PaintWater { .. }
                    | TerrainBrushKind::AddWater
                    | TerrainBrushKind::RemoveWater,
                ) => GeneralEconomyTransaction::WaterChange,
                Some(
                    TerrainBrushKind::Raise
                    | TerrainBrushKind::Lower
                    | TerrainBrushKind::Smooth
                    | TerrainBrushKind::Flatten { .. },
                ) => GeneralEconomyTransaction::Terraform,
            },
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
            preview: preview_entity,
        });
        break;
    }
}
