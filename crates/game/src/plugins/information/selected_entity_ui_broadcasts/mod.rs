use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::{
        action::{presentation::UiPresentationAction, UiActionRecord},
        document::UiDocumentRole,
    },
    world_definitions::world_objects::WorldObjectSelectedUiOperation,
};

use super::entity_selection_types::{Inspectable, SelectedEntity};
use crate::{
    assets::{
        ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
        world_definitions::{
            world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
            world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
        },
    },
    plugins::ui::{
        authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot, UiNodeId},
        authored_ui_presentation_action_application::RequestUiPresentationAction,
        ui_document_lifecycle_contracts::ShowUiRole,
    },
};

#[derive(Default)]
pub(super) struct PendingSelectedEntityUiBroadcasts {
    subject: Option<Entity>,
    next_broadcast: usize,
    waiting_for_role: Option<UiDocumentRole>,
    warned_missing_target: bool,
}

pub(super) fn dispatch_selected_entity_ui_broadcasts(
    selected: Res<SelectedEntity>,
    inspectables: Query<&Inspectable>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<(Entity, &UiDocumentRoot, &ChildOf)>,
    nodes: Query<(&UiNodeId, &UiDocumentOwner)>,
    mut pending: Local<PendingSelectedEntityUiBroadcasts>,
    mut requests: MessageWriter<RequestUiPresentationAction>,
    mut show: MessageWriter<ShowUiRole>,
) {
    if pending.subject != selected.0 {
        *pending = PendingSelectedEntityUiBroadcasts {
            subject: selected.0,
            ..default()
        };
    }
    let Some(subject) = pending.subject else {
        return;
    };
    let Some(definitions) = definitions.get(&definition_assets) else {
        return;
    };
    let Some(definition) = inspectables
        .get(subject)
        .ok()
        .and_then(|inspectable| definitions.find_object(inspectable.definition))
    else {
        return;
    };
    let Some(lifecycle_owner) = roots.iter().find_map(|(_, root, parent)| {
        documents
            .get(&root.document)
            .filter(|document| document.canonical_ui_document().role == UiDocumentRole::InGameHud)
            .map(|_| parent.parent())
    }) else {
        return;
    };
    while let Some(broadcast) = definition
        .selected_ui_broadcasts
        .get(pending.next_broadcast)
    {
        let target = nodes.iter().find_map(|(id, owner)| {
            let (_, root, parent) = roots.get(owner.0).ok()?;
            if parent.parent() != lifecycle_owner {
                return None;
            }
            let document = documents.get(&root.document)?;
            let node = document
                .canonical_ui_document()
                .nodes
                .get(id.index as usize)?;
            (node
                .name
                .trim()
                .eq_ignore_ascii_case(broadcast.target_name.trim()))
            .then_some((owner.0, id.id))
        });
        let Some((document_root, target_node)) = target else {
            let missing_role = documents.iter().find_map(|(_, document)| {
                let document = document.canonical_ui_document();
                document
                    .nodes
                    .iter()
                    .any(|node| node.name.trim().eq_ignore_ascii_case(broadcast.target_name.trim()))
                    .then_some(document.role)
            }).or_else(|| {
                documents.iter().find_map(|(_, document)| {
                    document.canonical_ui_document().nodes.iter()
                        .flat_map(|node| &node.actions)
                        .find_map(|record| {
                            let UiActionRecord::Presentation(record) = record else { return None; };
                            let (role, target) = match &record.action {
                                UiPresentationAction::SetDocumentNodeVisible { document_role, target_node, .. }
                                | UiPresentationAction::SetTargetNodeSelectionAndTimedSequencesActive {
                                    document_role: Some(document_role), target_node, ..
                                } => (*document_role, *target_node),
                                _ => return None,
                            };
                            (role.node_id(&broadcast.target_name) == target).then_some(role)
                        })
                })
            });
            if missing_role.is_none() && !pending.warned_missing_target {
                warn!(
                    target = broadcast.target_name,
                    "selected entity UI target is not loaded"
                );
                pending.warned_missing_target = true;
            }
            if let Some(role) = missing_role.filter(|role| pending.waiting_for_role != Some(*role))
            {
                show.write(ShowUiRole {
                    role,
                    owner: lifecycle_owner,
                });
                pending.waiting_for_role = Some(role);
            }
            return;
        };
        let action = match broadcast.operation {
            WorldObjectSelectedUiOperation::SetVisible(visible) => {
                UiPresentationAction::SetTargetNodeVisible {
                    target_node,
                    visible,
                }
            }
            WorldObjectSelectedUiOperation::SetActive(active) => {
                UiPresentationAction::SetTargetNodeSelectionAndTimedSequencesActive {
                    document_role: None,
                    target_node,
                    active,
                }
            }
        };
        requests.write(RequestUiPresentationAction {
            document_root,
            action,
        });
        pending.next_broadcast += 1;
        pending.waiting_for_role = None;
        pending.warned_missing_target = false;
    }
}
