use super::*;

#[cfg(test)]
mod tests;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::ui_document::{
    action::presentation::UiPresentationAction,
    document::{UiDocument, UiDocumentRole},
    widget::UiWidgetRecord,
};
use openzt2_interface_state::information::catalogue_types::ScenarioDisabledCatalogueDefinitions;
use openzt2_interface_state::{
    input::input_types::{ActionRequest, ActionSource, GameAction},
    ui::{
        authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot, UiNodeId},
        ui_document_lifecycle_contracts::{InGameUiOwner, UiRoleRequests},
    },
};
use openzt2_ui::animation::UiShowHideAnimation;
use openzt2_ui::authored_ui_presentation_action_application::RequestUiPresentationAction;
use openzt2_zoo_state::audio::audio_playback_message_types::PlayAudioCue;
use std::collections::{BTreeMap, VecDeque};

#[derive(Resource, Default)]
pub(super) struct PendingScenarioUi {
    nodes: VecDeque<(Vec<String>, NodeOperation)>,
    pub(super) enabled: BTreeMap<String, bool>,
    pub(super) shown: BTreeMap<String, bool>,
    selection_settling_frames: u8,
}

/// Native script calls run a toggle's authored chain to the end before returning.
/// Here each link takes a frame, so the next call has to wait for it: tutorials
/// select a tab and deselect its parent in consecutive calls.
const SELECTION_SETTLING_FRAMES: u8 = 6;

/// Adds a text node's laid-out height to a show/hide animation's final height.
#[derive(Component)]
pub(super) struct AnimationHeightFollowsText {
    text: Entity,
    base_height: f32,
}

pub(super) fn fit_animation_heights_to_text(
    mut panels: Query<(
        &AnimationHeightFollowsText,
        &mut UiShowHideAnimation,
        &mut Node,
    )>,
    texts: Query<&ComputedNode>,
) {
    for (fit, mut animation, mut node) in &mut panels {
        let Ok(text) = texts.get(fit.text) else {
            continue;
        };
        let height = fit.base_height + text.size().y * text.inverse_scale_factor();
        let start = animation.rect_intervals[3].map_or(height, |[start, _]| start);
        if animation.rect_intervals[3] != Some([start, height]) {
            animation.rect_intervals[3] = Some([start, height]);
            if !animation.running && animation.forward {
                node.height = Val::Px(height);
            }
        }
    }
}

pub(super) fn clear_scenario_ui(
    mut pending: ResMut<PendingScenarioUi>,
    mut requests: MessageReader<ScenarioUiCommand>,
    mut disabled_catalogue: ResMut<ScenarioDisabledCatalogueDefinitions>,
) {
    disabled_catalogue.0.clear();
    *pending = Default::default();
    requests.clear();
}

pub(super) fn role_for(path: &[String]) -> Option<UiDocumentRole> {
    match path.first()?.trim().to_ascii_lowercase().as_str() {
        "scenario win  layout" => Some(UiDocumentRole::ScenarioWin),
        "scenario failed layout" => Some(UiDocumentRole::ScenarioFailed),
        "tutorial complete" => Some(UiDocumentRole::TutorialComplete),
        "generic ok layout" => Some(UiDocumentRole::GenericOk),
        "goals layout" | "goalpanel" | "goal panel" | "introduction tab" | "primary goals tab"
        | "show all goals" => Some(UiDocumentRole::Scenario),
        "tutorial layout"
        | "tutorial block out"
        | "tutorial info"
        | "tutorial text"
        | "tutorial heading text"
        | "tutorial image"
        | "tutorial ring"
        | "ring timer"
        | "continue tutorial"
        | "edge arrow layout" => Some(UiDocumentRole::Tutorial),
        _ => None,
    }
}

pub(super) fn resolve_node(document: &UiDocument, path: &[String]) -> Option<AssetId> {
    let mut parent = None;
    for name in path {
        if let Some(index) = name.strip_prefix(INDEX_PATH_STEP) {
            parent = Some(index.parse::<usize>().ok()?);
            continue;
        }
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
    parent
        .and_then(|index| document.nodes.get(index))
        .map(|node| node.id)
}

fn ancestors(
    document: &UiDocument,
    index: usize,
) -> impl Iterator<Item = &openzt2_game_data::ui_document::node::UiNodeDefinition> {
    std::iter::successors(document.nodes.get(index), |node| {
        document.nodes.get(node.parent as usize)
    })
    .skip(1)
    .take(document.nodes.len())
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
    (mut sounds, mut disabled_catalogue, mut research_durations, mut unlocks): (
        MessageWriter<PlayAudioCue>,
        ResMut<ScenarioDisabledCatalogueDefinitions>,
        MessageWriter<openzt2_zoo_state::progression::research_types::SetResearchDuration>,
        MessageWriter<
            openzt2_zoo_state::progression::unlock_types::ApplyCatalogueDefinitionUnlockRequest,
        >,
    ),
    mut ring: ResMut<TutorialRingTimer>,
    mut animations: Query<(
        Entity,
        &UiNodeId,
        &UiDocumentOwner,
        &mut UiShowHideAnimation,
    )>,
    all_nodes: Query<(Entity, &UiNodeId, &UiDocumentOwner)>,
    mut commands: Commands,
) {
    let mut changed_enabled = Vec::new();
    let mut changed_shown = Vec::new();
    for request in requests.read() {
        match request {
            ScenarioUiCommand::Node { path, operation } => {
                pending.nodes.push_back((path.clone(), operation.clone()))
            }
            ScenarioUiCommand::Enabled { name, enabled } => {
                pending.enabled.insert(name.to_ascii_lowercase(), *enabled);
                changed_enabled.push(name.to_ascii_lowercase());
            }
            ScenarioUiCommand::Shown { name, visible } => {
                pending.shown.insert(name.to_ascii_lowercase(), *visible);
                changed_shown.push(name.to_ascii_lowercase());
            }
            ScenarioUiCommand::ResearchDuration {
                definition,
                seconds,
            } => {
                research_durations.write(
                    openzt2_zoo_state::progression::research_types::SetResearchDuration {
                        definition: *definition,
                        seconds: *seconds,
                    },
                );
            }
            ScenarioUiCommand::UnlockDefinition(definition) => {
                let operation = commands.spawn_empty().id();
                unlocks.write(openzt2_zoo_state::progression::unlock_types::ApplyCatalogueDefinitionUnlockRequest {
                    operation,
                    definition: *definition,
                });
                commands.entity(operation).despawn();
            }
            ScenarioUiCommand::Sound(name) => {
                sounds.write(PlayAudioCue {
                    cue: AssetId::from_key(&openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key(name)),
                    emitter: None,
                    selection: 0,
                    priority: 0,
                    force_looped: false,
                });
            }
            ScenarioUiCommand::NextScenario(name) => {
                continuation.scenario = name
                    .as_deref()
                    .filter(|name| !name.is_empty())
                    .map(AssetId::from_key)
            }
            ScenarioUiCommand::SelectionMode => {
                commands.queue(|world: &mut World| {
                    world.write_message(openzt2_zoo_state::construction::construction_interaction_types::CancelConstruction);
                    if let Some(mut tool) = world.get_resource_mut::<openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool>() {
                        *tool = openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool::Inspect;
                    }
                });
            }
            ScenarioUiCommand::Select(entity) => {
                let entity = *entity;
                commands.queue(move |world: &mut World| {
                    world.write_message(
                        openzt2_interface_state::information::entity_selection_types::SelectionRequest {
                            entity,
                            source: ActionSource::System,
                        },
                    );
                });
            }
            ScenarioUiCommand::Overhead => {
                actions.write(ActionRequest {
                    action: GameAction::OverheadView,
                    source: ActionSource::System,
                });
            }
        }
    }
    if !changed_enabled.is_empty() || !changed_shown.is_empty() || !added_nodes.is_empty() {
        // Disabling a control disables everything beneath it, and a later
        // `UI_ENABLE` can bring one child back: the tutorials disable "Zoom and
        // Rotate" and then enable "Zoom In". A newer call on a parent replaces those.
        for (_, root) in &roots {
            let Some(asset) = documents.get(&root.document) else {
                continue;
            };
            let document = asset.canonical_ui_document();
            for (index, node) in document.nodes.iter().enumerate() {
                let key = node.name.to_ascii_lowercase();
                if !changed_enabled.contains(&key)
                    && ancestors(document, index).any(|ancestor| {
                        changed_enabled.contains(&ancestor.name.to_ascii_lowercase())
                    })
                {
                    pending.enabled.remove(&key);
                }
            }
        }
        // Restrictions also apply to controls created after the script ran.
        for (root_entity, root) in &roots {
            let Some(asset) = documents.get(&root.document) else {
                continue;
            };
            let document = asset.canonical_ui_document();
            for (index, node) in document.nodes.iter().enumerate() {
                let key = node.name.to_ascii_lowercase();
                let added = || {
                    added_nodes
                        .iter()
                        .any(|(id, owner)| owner.0 == root_entity && id.id == node.id)
                };
                let restriction = std::iter::once(node)
                    .chain(ancestors(document, index))
                    .find_map(|restricted| {
                        let key = restricted.name.to_ascii_lowercase();
                        pending.enabled.get(&key).map(|enabled| (key, *enabled))
                    });
                if let Some((restricted, enabled)) = restriction {
                    if changed_enabled.contains(&restricted) || added() {
                        presentation.write(RequestUiPresentationAction {
                            document_root: root_entity,
                            action: UiPresentationAction::SetTargetNodeInteractionEnabled {
                                target_node: node.id,
                                enabled,
                            },
                        });
                    }
                }
                // Pointer input stops at a disabled ancestor, and a child enabled
                // on its own has to work inside its disabled group. Its siblings
                // stay disabled through their own flags.
                if pending.enabled.get(&key) == Some(&true) && changed_enabled.contains(&key) {
                    for ancestor in ancestors(document, index) {
                        presentation.write(RequestUiPresentationAction {
                            document_root: root_entity,
                            action: UiPresentationAction::SetTargetNodeInteractionEnabled {
                                target_node: ancestor.id,
                                enabled: true,
                            },
                        });
                    }
                }
                if let Some(&visible) = pending.shown.get(&key) {
                    if changed_shown.contains(&key) || added() {
                        presentation.write(RequestUiPresentationAction {
                            document_root: root_entity,
                            action: UiPresentationAction::SetTargetNodeVisible {
                                target_node: node.id,
                                visible,
                            },
                        });
                    }
                }
            }
        }
    }
    // Scripts also address buy-panel rows, by the name of what each one sells.
    if !changed_enabled.is_empty() {
        disabled_catalogue.0 = pending
            .enabled
            .iter()
            .filter(|(_, enabled)| !**enabled)
            .map(|(name, _)| AssetId::from_key(name))
            .collect();
    }
    let count = pending.nodes.len();
    pending.selection_settling_frames = pending.selection_settling_frames.saturating_sub(1);
    for _ in 0..count {
        if pending.selection_settling_frames > 0 {
            break;
        }
        let Some((path, operation)) = pending.nodes.pop_front() else {
            break;
        };
        if matches!(operation, NodeOperation::MeasureText) {
            continue;
        }
        if path
            .last()
            .is_some_and(|name| name.eq_ignore_ascii_case("ring timer"))
        {
            if let NodeOperation::Selected(active) = operation {
                ring.active = active;
            }
            continue;
        }
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
                // Scripts prepare hidden surfaces without showing them: the
                // tutorial selects a goal tab long before the player opens the
                // goal panel. The tutorial's own layout is always present.
                match (&operation, role) {
                    (_, UiDocumentRole::Tutorial) | (NodeOperation::Visible(true), _) => {
                        roles.request(role, owner);
                    }
                    (NodeOperation::Visible(false), _) => continue,
                    _ => {}
                }
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
                pending.selection_settling_frames = SELECTION_SETTLING_FRAMES;
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
            NodeOperation::MeasureText => continue,
            NodeOperation::Modal(modal) => {
                UiPresentationAction::SetTargetNodeModal { target_node, modal }
            }
            NodeOperation::WindAnimation => {
                UiPresentationAction::WindTargetShowHideAnimationToCurrentDirectionBoundary {
                    target_node,
                }
            }
            NodeOperation::Position(position) => {
                UiPresentationAction::SetTargetNodeLayoutPosition {
                    target_node,
                    position,
                }
            }
            NodeOperation::Size(size) => {
                UiPresentationAction::SetTargetNodeLayoutSize { target_node, size }
            }
            NodeOperation::IconKey(key) => {
                let index = roots
                    .get(root)
                    .ok()
                    .and_then(|(_, root)| documents.get(&root.document))
                    .and_then(|asset| {
                        asset
                            .canonical_ui_document()
                            .nodes
                            .iter()
                            .find(|node| node.id == target_node)
                    })
                    .and_then(|node| match &node.widget {
                        UiWidgetRecord::MultiIcon { entries } => {
                            entries.iter().position(|entry| entry.key == key)
                        }
                        _ => None,
                    });
                let Some(index) = index else {
                    warn!(?path, "scenario UI icon key is not authored on its target");
                    continue;
                };
                UiPresentationAction::SetTargetNodeIntegerValue {
                    target_node,
                    integer_value: index as i32,
                }
            }
            NodeOperation::AnimationStart(rectangle) => {
                if let Some((.., mut animation)) = animations
                    .iter_mut()
                    .find(|(_, id, owner, _)| owner.0 == root && id.id == target_node)
                {
                    for (interval, start) in animation.rect_intervals.iter_mut().zip(rectangle) {
                        *interval = Some([start, interval.map_or(start, |[_, end]| end)]);
                    }
                }
                continue;
            }
            NodeOperation::AnimationEnd {
                rectangle,
                fit_text,
            } => {
                let Some((entity, .., mut animation)) = animations
                    .iter_mut()
                    .find(|(_, id, owner, _)| owner.0 == root && id.id == target_node)
                else {
                    continue;
                };
                for (interval, end) in animation.rect_intervals.iter_mut().zip(rectangle) {
                    *interval = Some([interval.map_or(end, |[start, _]| start), end]);
                }
                let text = fit_text
                    .and_then(|text| {
                        let document = documents
                            .get(&roots.get(root).ok()?.1.document)?
                            .canonical_ui_document();
                        resolve_node(document, &text)
                    })
                    .and_then(|id| {
                        all_nodes
                            .iter()
                            .find(|(_, node, owner)| owner.0 == root && node.id == id)
                            .map(|(text, ..)| text)
                    });
                match text {
                    Some(text) => {
                        commands.entity(entity).insert(AnimationHeightFollowsText {
                            text,
                            base_height: rectangle[3],
                        });
                    }
                    None => {
                        commands
                            .entity(entity)
                            .remove::<AnimationHeightFollowsText>();
                    }
                }
                continue;
            }
        };
        presentation.write(RequestUiPresentationAction {
            document_root: root,
            action,
        });
    }
}
