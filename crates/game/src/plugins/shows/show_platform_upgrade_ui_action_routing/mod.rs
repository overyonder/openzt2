use bevy::prelude::*;
use openzt2_game_data::ui_document::action::animal_shows::{UiShowAction, UiShowPlatformUpgrade};

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::construction::construction_edit_history_types::ConstructionEditHistory;
use crate::plugins::construction::construction_transaction_types::DomainAckStatus;
use crate::plugins::construction::construction_transaction_types::EditApplication;
use crate::plugins::construction::construction_transaction_types::EditCommitPhase;
use crate::plugins::construction::construction_transaction_types::EditCommitProgress;
use crate::plugins::construction::construction_transaction_types::EditTransaction;
use crate::plugins::construction::construction_transaction_types::TransactionState;
use crate::plugins::economy::money_types::Money;
use crate::plugins::information::entity_selection_types::SelectedEntity;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    show_editor_interaction_types::ShowPlatformDeletionPending,
    show_platform_upgrade_types::{
        InstalledShowPlatformUpgrade, InstalledShowPlatformUpgradeStage,
        SelectedShowPlatformUpgrade, ShowPlatformUpgradeEdit, ShowPlatformUpgradeEditOperation,
        ShowPlatformUpgradeKind,
    },
    show_stage_types::ShowStage,
};
use crate::plugins::ui::authored_ui_action_projection_components::UiShowActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

#[allow(clippy::too_many_arguments)]
pub(super) fn route_authored_show_platform_upgrade_actions_into_selection_edits_and_deletion(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    selected_entity: Res<SelectedEntity>,
    action_nodes: Query<(&UiShowActions, &UiDocumentOwner)>,
    document_roots: Query<(&UiDocumentRoot, Option<&ChildOf>)>,
    stages: Query<Option<&WorldMember>, With<ShowStage>>,
    installed_upgrades: Query<(
        Entity,
        &InstalledShowPlatformUpgrade,
        &InstalledShowPlatformUpgradeStage,
    )>,
    selected_upgrades: Query<&SelectedShowPlatformUpgrade>,
    pending_deletions: Query<&ShowPlatformDeletionPending>,
    mut edit_history: ResMut<ConstructionEditHistory>,
) {
    for activation in activations.read() {
        let Ok((authored_actions, document_owner)) = action_nodes.get(activation.node) else {
            continue;
        };
        let Ok((document_root, parent)) = document_roots.get(document_owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&document_root.document) else {
            continue;
        };
        let controller = parent.map_or(document_owner.0, ChildOf::parent);
        let selected_stage = selected_entity.0.filter(|entity| stages.contains(*entity));

        for authored_action in authored_actions.authored_action_records(document) {
            if activation.trigger != authored_action.trigger {
                continue;
            }
            match &authored_action.action {
                UiShowAction::PurchaseSelectedPlatformUpgrade => {
                    let Some(stage) = selected_stage else {
                        continue;
                    };
                    let (Ok(world_member), Ok(selected_upgrade)) =
                        (stages.get(stage), selected_upgrades.get(controller))
                    else {
                        continue;
                    };
                    let installed_upgrade = installed_upgrades
                        .iter()
                        .find(|(_, installed_upgrade, owning_stage)| {
                            owning_stage.0 == stage && installed_upgrade.0 == selected_upgrade.0
                        })
                        .map(|(entity, _, _)| entity);
                    let Some(platform_upgrade_policy) = active_world_definitions
                        .get(&world_definition_assets)
                        .and_then(|world_definitions| world_definitions.show_platform_upgrades())
                    else {
                        continue;
                    };
                    let (purchase_cost_cents, resale_value_cents) = match selected_upgrade.0 {
                        ShowPlatformUpgradeKind::Canopy => (
                            platform_upgrade_policy.canopy_cost_cents,
                            platform_upgrade_policy.canopy_resale_cents,
                        ),
                        ShowPlatformUpgradeKind::Television => (
                            platform_upgrade_policy.television_cost_cents,
                            platform_upgrade_policy.television_resale_cents,
                        ),
                    };
                    let (transaction_cost_cents, edit_operation) = if installed_upgrade.is_some() {
                        (
                            -resale_value_cents,
                            ShowPlatformUpgradeEditOperation::Remove,
                        )
                    } else {
                        (
                            purchase_cost_cents,
                            ShowPlatformUpgradeEditOperation::Install,
                        )
                    };
                    if transaction_cost_cents == 0 {
                        continue;
                    }
                    let Some(sequence) = edit_history.allocate_sequence() else {
                        continue;
                    };
                    let Some(world_member) = world_member else {
                        continue;
                    };
                    commands.spawn((
                        EditTransaction {
                            sequence,
                            state: TransactionState::Applied,
                            cost: Money(transaction_cost_cents),
                        },
                        EditCommitProgress {
                            phase: EditCommitPhase::AwaitingEconomy,
                            application: EditApplication::InitialCommit,
                            cost: Money(transaction_cost_cents),
                            terrain: DomainAckStatus::NotExpected,
                            topology: DomainAckStatus::NotExpected,
                            placement: DomainAckStatus::NotExpected,
                            shows: DomainAckStatus::Pending,
                        },
                        ShowPlatformUpgradeEdit {
                            stage,
                            kind: selected_upgrade.0,
                            operation: edit_operation,
                        },
                        *world_member,
                    ));
                }
                UiShowAction::SelectPlatformUpgrade { upgrade } => {
                    let selected_upgrade = match upgrade {
                        UiShowPlatformUpgrade::Television => ShowPlatformUpgradeKind::Television,
                        UiShowPlatformUpgrade::Canopy => ShowPlatformUpgradeKind::Canopy,
                    };
                    commands
                        .entity(controller)
                        .insert(SelectedShowPlatformUpgrade(selected_upgrade));
                }
                UiShowAction::ResolvePlatformDeletion { confirmed } => {
                    if let Ok(pending_deletion) = pending_deletions.get(controller) {
                        if *confirmed && stages.contains(pending_deletion.stage) {
                            commands.entity(pending_deletion.stage).despawn();
                        }
                        commands
                            .entity(controller)
                            .remove::<ShowPlatformDeletionPending>();
                    }
                }
                _ => {}
            }
        }
    }
}
