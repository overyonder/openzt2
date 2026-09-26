//! Authored foreign-document child visibility survives asynchronous projection.
use bevy::prelude::*;
use openzt2_game_data::{ui_document::document::UiDocumentRole, AssetId};

use super::{
    animation::UiShowHideAnimation,
    authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot, UiNodeId},
    ui_document_asset_load_failure::UiDocumentAssetLoadFailed,
};
use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

#[derive(Component)]
pub(super) struct PendingCrossDocumentNodeVisibility {
    role: UiDocumentRole,
    node: AssetId,
    visible: bool,
}

pub(super) fn queue_cross_document_node_visibility(
    commands: &mut Commands,
    lifecycle_owner: Entity,
    role: UiDocumentRole,
    node: AssetId,
    visible: bool,
) {
    // Commands retain authored event order, including show/hide before either
    // request entity is query-visible. A later hide cancels a still-loading show.
    commands.queue(move |world: &mut World| {
        if world.get_entity(lifecycle_owner).is_err() {
            return;
        }
        let existing = world
            .query::<(Entity, &PendingCrossDocumentNodeVisibility, &ChildOf)>()
            .iter(world)
            .find_map(|(entity, pending, parent)| {
                (parent.parent() == lifecycle_owner && pending.role == role && pending.node == node)
                    .then_some(entity)
            });
        let pending = PendingCrossDocumentNodeVisibility {
            role,
            node,
            visible,
        };
        if let Some(existing) = existing {
            world.entity_mut(existing).insert(pending);
        } else {
            world.spawn((pending, ChildOf(lifecycle_owner)));
        }
    });
}

pub(super) fn apply_pending_cross_document_node_visibility(
    mut commands: Commands,
    documents: Res<Assets<UiDocumentAsset>>,
    pending: Query<(Entity, &PendingCrossDocumentNodeVisibility, &ChildOf)>,
    roots: Query<(Entity, &UiDocumentRoot, &ChildOf)>,
    mut nodes: Query<(
        &UiNodeId,
        &UiDocumentOwner,
        &mut Visibility,
        Option<&mut UiShowHideAnimation>,
    )>,
    mut failures: MessageReader<UiDocumentAssetLoadFailed>,
) {
    let failed_owners: Vec<_> = failures.read().map(|failure| failure.owner).collect();
    for (request, pending, lifecycle_owner) in &pending {
        if failed_owners.contains(&lifecycle_owner.parent()) {
            commands.entity(request).despawn();
            continue;
        }
        let root = roots.iter().find_map(|(entity, root, parent)| {
            (parent.parent() == lifecycle_owner.parent()
                && documents
                    .get(&root.document)
                    .is_some_and(|document| document.canonical_ui_document().role == pending.role))
            .then_some(entity)
        });
        let Some(root) = root else {
            if !pending.visible {
                commands.entity(request).despawn();
            }
            continue;
        };
        let target = nodes
            .iter_mut()
            .find(|(id, owner, ..)| owner.0 == root && id.id == pending.node);
        if let Some((_, _, mut visibility, animation)) = target {
            let animated = animation.is_some();
            if let Some(mut animation) = animation {
                animation.start_authored_visibility_transition(pending.visible);
            }
            if pending.visible {
                *visibility = Visibility::Inherited;
            } else if !animated {
                *visibility = Visibility::Hidden;
            }
        } else {
            warn!(role = ?pending.role, node = ?pending.node,
                "authored cross-document visibility target absent from projected document");
        }
        commands.entity(request).despawn();
    }
}
