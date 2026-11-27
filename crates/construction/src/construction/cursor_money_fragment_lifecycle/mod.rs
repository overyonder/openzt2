use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::AssetId;

use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiNodeId;
use openzt2_interface_state::ui::ui_document_lifecycle_contracts::ShowUiDocument;

use crate::construction::cursor_money_types::CursorMoneyContainer;
use crate::construction::cursor_money_types::CursorMoneyPreviewFragmentOwner;
use crate::construction::cursor_money_types::CursorMoneySpendFragmentOwner;
use crate::construction::cursor_money_types::PendingCursorMoneySpendAnimation;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionCommitted;
use openzt2_zoo_state::economy::money_types::Money;

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

    let mut reusable_preview = preview_fragment_owners.single().ok();
    for committed_transaction in committed_construction_transactions.read() {
        if committed_transaction.cost == Money::ZERO {
            continue;
        }
        let spend = CursorMoneySpendFragmentOwner {
            cost: committed_transaction.cost,
            screen_position: committed_transaction.screen_position,
        };
        if let Some(preview_fragment_owner) = reusable_preview.take() {
            commands
                .entity(preview_fragment_owner)
                .remove::<CursorMoneyPreviewFragmentOwner>()
                .insert((spend, PendingCursorMoneySpendAnimation));
        } else {
            let spend_fragment_owner = commands
                .spawn((
                    spend,
                    PendingCursorMoneySpendAnimation,
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
        if let Some(preview_fragment_owner) = reusable_preview {
            commands.entity(preview_fragment_owner).despawn();
        }
    } else if reusable_preview.is_none() {
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
    mut activations: MessageReader<UiNodeActivated>,
    spend_fragment_owners: Query<
        (),
        (
            With<CursorMoneySpendFragmentOwner>,
            Without<PendingCursorMoneySpendAnimation>,
        ),
    >,
    roots: Query<&ChildOf, With<UiDocumentRoot>>,
) {
    for activation in activations.read() {
        if activation.trigger != UiTrigger::AnimationCompleted {
            continue;
        }
        if let Ok(parent) = roots.get(activation.node) {
            if spend_fragment_owners.contains(parent.parent()) {
                commands.entity(parent.parent()).try_despawn();
            }
        }
    }
}
