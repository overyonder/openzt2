use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::ui::{
    animation::UiShowHideAnimation, authored_ui_node_projection_components::UiDocumentRoot,
    authored_ui_node_projection_components::UiNodeId,
    ui_document_lifecycle_contracts::ShowUiDocument,
};

use super::construction_transaction_types::ConstructionCommitted;
use super::{
    construction_interaction_types::ConstructionPreview,
    cursor_money_types::{
        CursorMoneyContainer, CursorMoneyPreviewFragmentOwner, CursorMoneySpendFragmentOwner,
    },
};

const CURSOR_MONEY_FRAGMENT_SOURCE_PATH: &str = "ui/layout/cursormoney.xml";

/// Marks the exact source-composed original container by its stable node
/// identity. The dynamic child then uses the same authored 1024x768 coordinate
/// space as the HUD.
pub(super) fn identify_authored_cursor_money_container(
    mut commands: Commands,
    nodes: Query<(Entity, &UiNodeId), Added<UiNodeId>>,
) {
    let cursor_money_container_node_identifier =
        AssetId::from_key("ui/role/in-game-hud/node/cursor money container");
    for (entity, node) in &nodes {
        if node.id == cursor_money_container_node_identifier {
            commands.entity(entity).insert(CursorMoneyContainer);
        }
    }
}

/// Keeps one non-animated price at the live pointer while placement is active.
/// A successful transaction reuses that exact fragment as the frozen spend
/// animation, then the continuing preview receives a fresh fragment.
pub(super) fn reconcile_cursor_money_fragments(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    containers: Query<Entity, With<CursorMoneyContainer>>,
    previews: Query<(), With<ConstructionPreview>>,
    preview_fragment_owners: Query<Entity, With<CursorMoneyPreviewFragmentOwner>>,
    mut committed_construction_transactions: MessageReader<ConstructionCommitted>,
    mut show_ui_document: MessageWriter<ShowUiDocument>,
) {
    let Ok(cursor_money_container) = containers.single() else {
        return;
    };

    for committed_transaction in committed_construction_transactions.read() {
        if committed_transaction.cost.0 == 0 {
            continue;
        }
        if let Ok(preview_fragment_owner) = preview_fragment_owners.single() {
            commands
                .entity(preview_fragment_owner)
                .remove::<CursorMoneyPreviewFragmentOwner>()
                .insert(CursorMoneySpendFragmentOwner {
                    cost: committed_transaction.cost,
                    screen_position: committed_transaction.screen_position,
                    animation_started: false,
                });
        } else {
            let spend_fragment_owner = commands
                .spawn((
                    CursorMoneySpendFragmentOwner {
                        cost: committed_transaction.cost,
                        screen_position: committed_transaction.screen_position,
                        animation_started: false,
                    },
                    ChildOf(cursor_money_container),
                ))
                .id();
            show_ui_document.write(ShowUiDocument {
                document: asset_server.load(CURSOR_MONEY_FRAGMENT_SOURCE_PATH),
                owner: spend_fragment_owner,
            });
        }
    }

    if previews.is_empty() {
        for preview_fragment_owner in &preview_fragment_owners {
            commands.entity(preview_fragment_owner).despawn();
        }
    } else if preview_fragment_owners.is_empty() {
        let preview_fragment_owner = commands
            .spawn((
                CursorMoneyPreviewFragmentOwner,
                ChildOf(cursor_money_container),
            ))
            .id();
        show_ui_document.write(ShowUiDocument {
            document: asset_server.load(CURSOR_MONEY_FRAGMENT_SOURCE_PATH),
            owner: preview_fragment_owner,
        });
    }
}

/// Removes each dynamic spend child only after its authored reverse animation
/// reaches the start state and emits completion.
pub(super) fn remove_completed_cursor_money_spend_fragments(
    mut commands: Commands,
    spend_fragment_owners: Query<Entity, With<CursorMoneySpendFragmentOwner>>,
    roots: Query<(&ChildOf, &UiShowHideAnimation), With<UiDocumentRoot>>,
) {
    for (parent, animation) in &roots {
        if spend_fragment_owners.contains(parent.parent())
            && !animation.running
            && !animation.forward
            && animation.elapsed_ms == 0.0
        {
            commands.entity(parent.parent()).despawn();
        }
    }
}
