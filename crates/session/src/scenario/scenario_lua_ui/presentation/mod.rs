use super::*;

#[cfg(test)]
mod tests;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::ui_document::{
    action::presentation::UiPresentationAction,
    document::{UiDocument, UiDocumentRole},
};
use openzt2_interface_state::{
    input::input_types::{ActionRequest, ActionSource, GameAction},
    ui::{
        authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot, UiNodeId},
        ui_document_lifecycle_contracts::{InGameUiOwner, UiRoleRequests},
    },
};
use openzt2_ui::authored_ui_presentation_action_application::RequestUiPresentationAction;
use std::collections::{BTreeMap, VecDeque};

#[derive(Resource, Default)]
pub(super) struct PendingScenarioUi {
    nodes: VecDeque<(Vec<String>, NodeOperation)>,
    pub(super) enabled: BTreeMap<String, bool>,
}

pub(super) fn clear_scenario_ui(
    mut pending: ResMut<PendingScenarioUi>,
    mut requests: MessageReader<ScenarioUiCommand>,
) {
    *pending = Default::default();
    requests.clear();
}

fn role_for(path: &[String]) -> Option<UiDocumentRole> {
    match path.first()?.trim().to_ascii_lowercase().as_str() {
        "scenario win  layout" => Some(UiDocumentRole::ScenarioWin),
        "scenario failed layout" => Some(UiDocumentRole::ScenarioFailed),
        "generic ok layout" => Some(UiDocumentRole::GenericOk),
        "goals layout" | "goalpanel" | "goal panel" | "introduction tab" | "primary goals tab"
        | "show all goals" => Some(UiDocumentRole::Scenario),
        _ => None,
    }
}

fn resolve_node(document: &UiDocument, path: &[String]) -> Option<AssetId> {
    let mut parent = None;
    for name in path {
        let mut matches = document.nodes.iter().enumerate().filter(|(index, node)| {
            if !openzt2_game_assets::source_document::source_document_semantic_name::source_document_names_are_semantically_equal(&node.name, name) {
                return false;
            }
            let Some(parent) = parent else { return true };
            let mut cursor = *index;
            while let Some(node) = document.nodes.get(cursor) {
                let ancestor = node.parent as usize;
                if ancestor == parent {
                    return true;
                }
                if ancestor == cursor || ancestor >= document.nodes.len() {
                    break;
                }
                cursor = ancestor;
            }
            false
        });
        let (index, _) = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        parent = Some(index);
    }
    parent.map(|index| document.nodes[index].id)
}

pub(super) fn apply_scenario_ui_commands(
    mut requests: MessageReader<ScenarioUiCommand>,
    mut pending: ResMut<PendingScenarioUi>,
    mut continuation: ResMut<ScenarioContinuation>,
    mut roles: ResMut<UiRoleRequests>,
    owners: Query<Entity, With<InGameUiOwner>>,
    roots: Query<(Entity, &UiDocumentRoot)>,
    added_nodes: Query<(&UiNodeId, &UiDocumentOwner), Added<UiNodeId>>,
    documents: Res<Assets<UiDocumentAsset>>,
    mut presentation: MessageWriter<RequestUiPresentationAction>,
    mut actions: MessageWriter<ActionRequest>,
) {
    let mut changed_enabled = Vec::new();
    for request in requests.read() {
        match request {
            ScenarioUiCommand::Node { path, operation } => {
                pending.nodes.push_back((path.clone(), operation.clone()))
            }
            ScenarioUiCommand::Enabled { name, enabled } => {
                pending.enabled.insert(name.to_ascii_lowercase(), *enabled);
                changed_enabled.push(name.to_ascii_lowercase());
            }
            ScenarioUiCommand::NextScenario(name) => {
                continuation.scenario = name
                    .as_deref()
                    .filter(|name| !name.is_empty())
                    .map(AssetId::from_key)
            }
            ScenarioUiCommand::Overhead => {
                actions.write(ActionRequest {
                    action: GameAction::OverheadView,
                    source: ActionSource::System,
                });
            }
        }
    }
    // Restrictions also apply to controls created after the script ran.
    for (root_entity, root) in &roots {
        let Some(asset) = documents.get(&root.document) else {
            continue;
        };
        for node in &asset.canonical_ui_document().nodes {
            let key = node.name.to_ascii_lowercase();
            let Some(&enabled) = pending.enabled.get(&key) else {
                continue;
            };
            if changed_enabled.contains(&key)
                || added_nodes
                    .iter()
                    .any(|(id, owner)| owner.0 == root_entity && id.id == node.id)
            {
                presentation.write(RequestUiPresentationAction {
                    document_root: root_entity,
                    action: UiPresentationAction::SetTargetNodeInteractionEnabled {
                        target_node: node.id,
                        enabled,
                    },
                });
            }
        }
    }
    let count = pending.nodes.len();
    for _ in 0..count {
        let Some((path, operation)) = pending.nodes.pop_front() else {
            break;
        };
        let role = role_for(&path);
        let target = roots.iter().find_map(|(entity, root)| {
            let document = documents.get(&root.document)?.canonical_ui_document();
            if role.is_some_and(|role| role != document.role) {
                return None;
            }
            resolve_node(document, &path).map(|id| (entity, id))
        });
        let Some((root, target_node)) = target else {
            if let (Some(role), Ok(owner)) = (role, owners.single()) {
                if roots.iter().any(|(_, root)| {
                    documents
                        .get(&root.document)
                        .is_some_and(|asset| asset.canonical_ui_document().role == role)
                }) {
                    warn!(
                        ?path,
                        ?role,
                        "scenario UI target is absent or ambiguous in its authored document"
                    );
                    continue;
                }
                roles.request(role, owner);
                pending.nodes.push_back((path, operation));
            } else {
                warn!(?path, "scenario UI target has no projected document");
            }
            continue;
        };
        let action = match operation {
            NodeOperation::Visible(visible) => UiPresentationAction::SetTargetNodeVisible {
                target_node,
                visible,
            },
            NodeOperation::Selected(active) => {
                UiPresentationAction::SetTargetNodeSelectionAndTimedSequencesActive {
                    document_role: None,
                    target_node,
                    active,
                }
            }
            NodeOperation::Press => {
                UiPresentationAction::ActivateTargetNodeWithPress { target_node }
            }
            NodeOperation::Localized(localization_key) => {
                UiPresentationAction::SetTargetNodeTextFromLocalizationKey {
                    target_node,
                    localization_key,
                }
            }
            NodeOperation::Text(text) => {
                UiPresentationAction::SetTargetNodeTextLiteral { target_node, text }
            }
            // Goal rows are already projected from the canonical objective collection.
            NodeOperation::RefreshGoals => continue,
        };
        presentation.write(RequestUiPresentationAction {
            document_root: root,
            action,
        });
    }
}
