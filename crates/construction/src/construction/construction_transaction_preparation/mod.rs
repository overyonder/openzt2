use super::{
    construction_history::WorldConstructionHistory,
    construction_lifecycle::{ConstructionActive, Preparing},
};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::economy::general_economy_types::GeneralEconomyTransaction;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::terrain::terrain_brush_types::{TerrainBrushKind, TerrainBrushPreview};

use openzt2_zoo_state::construction::construction_interaction_types::CommitConstruction;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionFeedbackOrigin;
use openzt2_zoo_state::construction::construction_transaction_types::EditTransaction;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareConstruction;
use openzt2_zoo_state::construction::construction_transaction_types::TransactionState;

#[derive(Component)]
pub struct SubmittedConstruction {
    pub source: Entity,
    pub tool: ConstructionTool,
    pub preview: ConstructionPreview,
}

#[derive(Component)]
pub struct SubmittedPreview<T: Component + Clone>(pub T);

#[derive(SystemSet, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ConstructionSubmissionSet;

pub fn capture_submitted_preview<T: Component + Clone>(
    submissions: Query<(Entity, &SubmittedConstruction), Added<SubmittedConstruction>>,
    previews: Query<&T>,
    mut commands: Commands,
) {
    for (transaction, submission) in submissions {
        if let Ok(preview) = previews.get(submission.source) {
            commands
                .entity(transaction)
                .insert(SubmittedPreview(preview.clone()));
        }
    }
}

pub(super) fn prepare_submitted_construction(
    submitted: Query<(Entity, &SubmittedConstruction), Added<SubmittedConstruction>>,
    mut requests: MessageWriter<PrepareConstruction>,
) {
    for (transaction, submission) in submitted {
        requests.write(PrepareConstruction {
            transaction,
            preview: submission.source,
        });
    }
}

pub(super) fn clear_submitted_construction(
    submitted: Query<Entity, With<SubmittedConstruction>>,
    mut commands: Commands,
) {
    for transaction in submitted {
        commands
            .entity(transaction)
            .remove::<SubmittedConstruction>();
    }
}

pub fn clear_submitted_preview<T: Component + Clone>(
    submitted: Query<Entity, With<SubmittedPreview<T>>>,
    mut commands: Commands,
) {
    for transaction in submitted {
        commands.entity(transaction).remove::<SubmittedPreview<T>>();
    }
}

/// Opens one construction transaction only from the exact still-live valid
/// preview. The confirming pointer coordinate is retained solely for feedback
/// after the domain and economy transaction has fully committed.
pub fn begin_construction_transaction_from_valid_preview(
    mut commands: Commands,
    mut construction_commit_requests: MessageReader<CommitConstruction>,
    tool: Res<ConstructionTool>,
    previews: Query<(
        &ConstructionPreview,
        &WorldMember,
        Option<&TerrainBrushPreview>,
    )>,
    pending_transactions: Query<(), With<ConstructionActive>>,
    mut world_histories: Query<&mut WorldConstructionHistory, With<WorldRoot>>,
    primary_windows: Query<&Window, With<PrimaryWindow>>,
) {
    if !pending_transactions.is_empty() {
        construction_commit_requests.clear();
        return;
    }
    let mut requests = construction_commit_requests.read();
    for request in requests.by_ref() {
        let preview_entity = request.preview;
        let Ok((preview, world_member, terrain_brush)) = previews.get(preview_entity) else {
            continue;
        };
        if !matches!(preview.validity, PlacementValidity::Valid { .. }) {
            continue;
        }
        let Ok(mut history) = world_histories.get_mut(world_member.root) else {
            continue;
        };
        let Some(transaction_sequence) = history.allocate_sequence() else {
            continue;
        };
        let mut transaction = commands.spawn((
            EditTransaction {
                sequence: transaction_sequence,
                state: TransactionState::Applied,
                cost: Money(0),
            },
            Preparing,
            SubmittedConstruction {
                source: preview_entity,
                tool: *tool,
                preview: preview.clone(),
            },
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
        break;
    }
    requests.for_each(drop);
}
