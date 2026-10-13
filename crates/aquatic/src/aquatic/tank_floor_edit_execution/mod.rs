use bevy::prelude::*;

use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_transaction_types::DomainAckStatus;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitPhase;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitProgress;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareConstruction;
use openzt2_zoo_state::construction::construction_transaction_types::RollbackContext;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::habitat::habitat_types::HabitatSummary;

use super::{
    aquatic_simulation_types::{Tank, TankGeometry, TankSurfaceHydrated},
    tank_floor_edit_types::{
        CommitTankFloorEdit, TankFloorEdit, TankFloorEditAcknowledged,
        TankFloorEditPreparationRejected, TankFloorEditPrepared, TankFloorGesture,
    },
    tank_floor_edit_validation::tank_floor_edit_matches_authored_depth_bounds,
};

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn prepare_tank_floor_construction_edits(
    mut commands: Commands,
    mut requests: MessageReader<PrepareConstruction>,
    previews: Query<(&TankFloorEdit, &TankFloorGesture, &WorldMember)>,
    transactions: Query<(&EditCommitProgress, &WorldMember)>,
    tanks: Query<
        (
            &Tank,
            &TankGeometry,
            &HabitatRegion,
            &HabitatSummary,
            &WorldMember,
        ),
        With<TankSurfaceHydrated>,
    >,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut prepared: MessageWriter<TankFloorEditPrepared>,
    mut rejected: MessageWriter<TankFloorEditPreparationRejected>,
) {
    for request in requests.read() {
        let Ok((edit, gesture, preview_member)) = previews.get(request.preview) else {
            continue;
        };
        let valid = transactions
            .get(request.transaction)
            .is_ok_and(|(progress, member)| {
                progress.phase == EditCommitPhase::Preparing
                    && member.root == preview_member.root
                    && gesture.pending
                    && tanks.get(edit.tank).is_ok_and(
                        |(tank, geometry, region, summary, tank_member)| {
                            tank.definition == edit.definition
                                && *geometry == edit.before
                                && tank_member.root == member.root
                                && !summary.boundary_is_breached
                                && region.area_square_metres == geometry.area
                        },
                    )
            })
            && active_definitions
                .get(&definitions)
                .is_some_and(|definitions| {
                    definitions.tank_edit_policy().is_some()
                        && edit.before != edit.after
                        && tank_floor_edit_matches_authored_depth_bounds(
                            edit,
                            edit.before,
                            definitions,
                        )
                        && tank_floor_edit_matches_authored_depth_bounds(
                            edit,
                            edit.after,
                            definitions,
                        )
                });
        if valid {
            commands.entity(request.transaction).insert(*edit);
            prepared.write(TankFloorEditPrepared {
                transaction: request.transaction,
                cost: Money(0),
            });
        } else {
            rejected.write(TankFloorEditPreparationRejected {
                transaction: request.transaction,
                reason: PlacementFailure::InvalidHabitat,
            });
        }
        commands.entity(request.preview).despawn();
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn apply_tank_floor_construction_edits(
    mut requests: MessageReader<CommitTankFloorEdit>,
    transactions: Query<(
        &TankFloorEdit,
        &EditCommitProgress,
        &WorldMember,
        Option<&RollbackContext>,
    )>,
    mut tanks: Query<
        (
            &Tank,
            &mut TankGeometry,
            &HabitatRegion,
            &HabitatSummary,
            &WorldMember,
        ),
        With<TankSurfaceHydrated>,
    >,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut acknowledged: MessageWriter<TankFloorEditAcknowledged>,
) {
    for request in requests.read() {
        let Ok((edit, progress, member, rollback)) = transactions.get(request.transaction) else {
            continue;
        };
        if progress.application != request.application {
            continue;
        }
        let forward = match request.application {
            EditApplication::InitialCommit | EditApplication::Redo => true,
            EditApplication::Undo => false,
            EditApplication::FailureRollback => {
                let Some(rollback) = rollback else {
                    continue;
                };
                rollback.failed_application == EditApplication::Undo
            }
        };
        let authorized = if request.application == EditApplication::FailureRollback {
            progress.phase == EditCommitPhase::RollingBack
                && progress.aquatic
                    == if forward {
                        DomainAckStatus::Reverted
                    } else {
                        DomainAckStatus::Applied
                    }
        } else {
            progress.phase == EditCommitPhase::Applying
                && progress.aquatic == DomainAckStatus::Pending
        };
        if !authorized {
            continue;
        }
        let (expected, replacement) = if forward {
            (edit.before, edit.after)
        } else {
            (edit.after, edit.before)
        };
        let accepted = tanks.get_mut(edit.tank).is_ok_and(
            |(tank, mut geometry, region, summary, tank_member)| {
                if tank.definition != edit.definition
                    || *geometry != expected
                    || tank_member.root != member.root
                    || summary.boundary_is_breached
                    || region.area_square_metres != expected.area
                    || !active_definitions
                        .get(&definitions)
                        .is_some_and(|definitions| {
                            tank_floor_edit_matches_authored_depth_bounds(
                                edit,
                                replacement,
                                definitions,
                            )
                        })
                {
                    return false;
                }
                *geometry = replacement;
                true
            },
        );
        acknowledged.write(TankFloorEditAcknowledged {
            transaction: request.transaction,
            application: request.application,
            accepted,
        });
    }
}
