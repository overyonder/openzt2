use std::collections::BTreeMap;

use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_data::{
    ui_document::{
        action::{presentation::UiPresentationAction, UiActionRecord},
        document::UiDocumentRole,
    },
    world_definitions::world_objects::WorldObjectSelectedUiOperation,
    AssetId,
};

use openzt2_game_assets::{
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_interface_state::information::entity_selection_types::{
    Inspectable, SelectedSubject, SelectionChanged,
};
use openzt2_interface_state::ui::{
    authored_ui_node_projection_components::{UiDocumentMembers, UiDocumentRoot, UiNodeId},
    ui_document_lifecycle_contracts::{InGameUiOwner, ShowUiRole},
};
use openzt2_ui::authored_ui_presentation_action_application::RequestUiPresentationAction;

#[derive(Component, Default)]
pub(super) struct PendingSelectedEntityUiBroadcasts {
    next: usize,
    waiting_for_role: Option<UiDocumentRole>,
}

#[derive(Component, PartialEq, Eq)]
pub(super) struct SelectedEntityUiBroadcastTargets(BTreeMap<String, AssetId>);

pub(super) fn index_projected_selected_entity_ui_broadcast_targets(
    documents: Res<Assets<UiDocumentAsset>>,
    mut roots: Query<(
        Entity,
        Ref<UiDocumentRoot>,
        Ref<UiDocumentMembers>,
        Option<&mut SelectedEntityUiBroadcastTargets>,
    )>,
    nodes: Query<&UiNodeId>,
    mut commands: Commands,
) {
    for (owner, root, members, current) in &mut roots {
        if current.is_some()
            && !root.is_changed()
            && !members.is_changed()
            && !documents.is_changed()
        {
            continue;
        }
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let authored_nodes = &document.canonical_ui_document().nodes;
        let mut targets = BTreeMap::new();
        for node in members.iter().filter_map(|entity| nodes.get(entity).ok()) {
            if let Some(authored) = authored_nodes.get(node.index as usize) {
                let name = authored.name.trim();
                if !name.is_empty() {
                    targets.entry(name.to_ascii_lowercase()).or_insert(node.id);
                }
            }
        }
        let bindings = SelectedEntityUiBroadcastTargets(targets);
        if let Some(mut current) = current {
            current.set_if_neq(bindings);
        } else {
            commands.entity(owner).insert(bindings);
        }
    }
}

pub(super) fn begin_selected_entity_ui_broadcasts(
    mut selections: MessageReader<SelectionChanged>,
    mut owners: Query<
        (
            Entity,
            Option<Ref<SelectedSubject>>,
            Option<&mut PendingSelectedEntityUiBroadcasts>,
        ),
        With<InGameUiOwner>,
    >,
    mut commands: Commands,
) {
    let selection_changed = selections.read().count() != 0;
    for (owner, subject, pending) in &mut owners {
        let Some(subject) = subject else {
            if pending.is_some() {
                commands
                    .entity(owner)
                    .remove::<PendingSelectedEntityUiBroadcasts>();
            }
            continue;
        };
        if selection_changed || subject.is_changed() {
            if let Some(mut pending) = pending {
                *pending = PendingSelectedEntityUiBroadcasts::default();
            } else {
                commands
                    .entity(owner)
                    .insert(PendingSelectedEntityUiBroadcasts::default());
            }
        }
    }
}

pub(super) fn dispatch_selected_entity_ui_broadcasts(
    mut owners: Query<
        (
            Entity,
            &SelectedSubject,
            Option<Ref<Children>>,
            &mut PendingSelectedEntityUiBroadcasts,
        ),
        With<InGameUiOwner>,
    >,
    inspectables: Query<Ref<Inspectable>>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    documents: Res<Assets<UiDocumentAsset>>,
    targets: Query<Ref<SelectedEntityUiBroadcastTargets>>,
    mut requests: MessageWriter<RequestUiPresentationAction>,
    mut show: MessageWriter<ShowUiRole>,
    mut commands: Commands,
) {
    let definitions_changed = definitions.is_changed() || assets.is_changed();
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    for (owner, subject, children, mut pending) in &mut owners {
        let Ok(inspectable) = inspectables.get(subject.0) else {
            commands
                .entity(owner)
                .remove::<PendingSelectedEntityUiBroadcasts>();
            continue;
        };
        let documents_changed = documents.is_changed()
            || children.as_ref().is_some_and(|children| {
                children.is_changed()
                    || children.iter().any(|entity| {
                        targets
                            .get(entity)
                            .is_ok_and(|targets| targets.is_changed())
                    })
            });
        if !pending.is_changed()
            && !definitions_changed
            && !documents_changed
            && !inspectable.is_changed()
        {
            continue;
        }
        let Some(definition) = definitions.find_object(inspectable.definition) else {
            continue;
        };
        while let Some(broadcast) = definition.selected_ui_broadcasts.get(pending.next) {
            let target_name = broadcast.target_name.trim().to_ascii_lowercase();
            let target = children
                .as_ref()
                .into_iter()
                .flat_map(|children| children.iter())
                .find_map(|entity| {
                    targets
                        .get(entity)
                        .ok()?
                        .0
                        .get(&target_name)
                        .copied()
                        .map(|target| (entity, target))
                });
            let Some((document_root, target_node)) = target else {
                if let Some(role) =
                    authored_role_for_broadcast_target(&documents, target_name.as_str())
                {
                    if pending.waiting_for_role != Some(role) {
                        show.write(ShowUiRole { role, owner });
                        pending.waiting_for_role = Some(role);
                    }
                }
                break;
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
            pending.next += 1;
            pending.waiting_for_role = None;
        }
        if pending.next >= definition.selected_ui_broadcasts.len() {
            commands
                .entity(owner)
                .remove::<PendingSelectedEntityUiBroadcasts>();
        }
    }
}

fn authored_role_for_broadcast_target(
    documents: &Assets<UiDocumentAsset>,
    name: &str,
) -> Option<UiDocumentRole> {
    documents
        .iter()
        .map(|(_, document)| document.canonical_ui_document())
        .find_map(|document| {
            document
                .nodes
                .iter()
                .any(|node| node.name.trim().eq_ignore_ascii_case(name))
                .then_some(document.role)
        })
        .or_else(|| {
            documents.iter().find_map(|(_, document)| {
                document
                    .canonical_ui_document()
                    .nodes
                    .iter()
                    .flat_map(|node| &node.actions)
                    .find_map(|record| {
                        let UiActionRecord::Presentation(record) = record else {
                            return None;
                        };
                        let (role, target) = match &record.action {
                    UiPresentationAction::SetDocumentNodeVisible { document_role, target_node, .. }
                    | UiPresentationAction::SetTargetNodeSelectionAndTimedSequencesActive {
                        document_role: Some(document_role), target_node, ..
                    } => (*document_role, *target_node),
                    _ => return None,
                };
                        (role.node_id(name) == target).then_some(role)
                    })
            })
        })
}
