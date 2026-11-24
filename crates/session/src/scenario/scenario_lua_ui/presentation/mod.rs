use super::*;
use bevy::ecs::{relationship::RelationshipTarget, system::SystemParam};
use openzt2_interface_state::ui::{
    authored_ui_action_projection_components::UiPresentationActions,
    authored_ui_activation_contracts::UiNodeActivated,
    authored_ui_node_projection_components::UiDocumentMembers,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::scenario::{
    scenario_session_types::{ScenarioTutorialRuleActive, SetScenarioFencePlacementMode},
    timed_scenario_ui_actions::{PendingScenarioUiSelection, ScenarioUiSelectionCompleted},
};

#[cfg(test)]
mod tests;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::ui_document::{
    action::presentation::UiPresentationAction,
    document::{UiDocument, UiDocumentRole},
    widget::UiWidgetRecord,
};
use openzt2_interface_state::information::catalogue_types::ScenarioDisabledCatalogueDefinitions;
use openzt2_interface_state::information::{
    entity_selection_types::SelectionRequest,
    zoo_message_types::{ZooMessage, ZooMessages},
};
use openzt2_interface_state::{
    input::input_types::{ActionRequest, ActionSource, GameAction},
    ui::{
        authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot, UiNodeId},
        ui_document_lifecycle_contracts::{InGameUiOwner, ShowUiRole},
    },
};
use openzt2_ui::animation::UiShowHideAnimation;
use openzt2_ui::authored_ui_presentation_action_application::RequestUiPresentationAction;
use openzt2_zoo_state::audio::audio_playback_message_types::PlayAudioCue;
use openzt2_zoo_state::{
    aquatic::tank_geometry_edit_types::SetTankWallHeights,
    construction::{
        construction_interaction_types::CancelConstruction,
        construction_tool_and_placement_policy_types::{
            ConstructionPlacementPolicy, ConstructionTool, FencePlacementShape,
            SelectConstructionTool,
        },
    },
};
use std::collections::{BTreeMap, VecDeque};

#[derive(Component, Default)]
pub(in crate::scenario) struct ScenarioUiState {
    nodes: VecDeque<(Vec<String>, NodeOperation)>,
    hidden: VecDeque<(Vec<String>, NodeOperation)>,
    fence_placement_mode: Option<SetScenarioFencePlacementMode>,
    pub(super) enabled: BTreeMap<String, bool>,
    pub(super) shown: BTreeMap<String, bool>,
    pub(in crate::scenario) continuation: Option<AssetId>,
    selection: Option<ScenarioUiSelectionCompleted>,
    cached: BTreeMap<(Entity, AssetId), CachedPanel>,
}

#[derive(Default)]
struct CachedPanel {
    visible: bool,
    draft: Vec<UiPresentationAction>,
    entries: VecDeque<Vec<UiPresentationAction>>,
}

#[derive(Message, Debug, Clone, Copy)]
pub(in crate::scenario) enum ScenarioConstructionRequest {
    Select(ConstructionTool),
    Place(AssetId),
    Mode(SetScenarioFencePlacementMode),
    ReturnToSelection,
}

#[derive(Component)]
pub(super) struct ScenarioUiDocuments {
    _documents: Vec<Handle<UiDocumentAsset>>,
}

pub(super) fn preload_scenario_ui_documents(
    mut commands: Commands,
    server: Res<AssetServer>,
    worlds: Query<Entity, (With<WorldRoot>, Without<ScenarioUiDocuments>)>,
) {
    use openzt2_game_assets::ui_document::ui_document_role_asset_path_selection::select_bevy_asset_path_for_canonical_ui_document_role;
    for owner in &worlds {
        let documents = [
            UiDocumentRole::Tutorial,
            UiDocumentRole::Scenario,
            UiDocumentRole::ChallengeOffer,
        ]
        .into_iter()
        .filter_map(select_bevy_asset_path_for_canonical_ui_document_role)
        .map(|path| server.load(path))
        .collect::<Vec<Handle<UiDocumentAsset>>>();
        commands.entity(owner).insert(ScenarioUiDocuments {
            _documents: documents,
        });
    }
}

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

pub(super) fn clear_scenario_ui(mut requests: MessageReader<ScenarioUiCommand>) {
    requests.clear();
}

fn role_for(documents: &Assets<UiDocumentAsset>, path: &[String]) -> Option<UiDocumentRole> {
    let mut roles = documents.iter().filter_map(|(_, asset)| {
        let document = asset.canonical_ui_document();
        resolve_node(document, path).map(|_| document.role)
    });
    let first = roles.next()?;
    roles.all(|role| role == first).then_some(first)
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

#[derive(SystemParam)]
pub(super) struct ScenarioUiProjectionEvents<'w, 's> {
    activations: MessageReader<'w, 's, UiNodeActivated>,
    document_assets: MessageReader<'w, 's, AssetEvent<UiDocumentAsset>>,
    selection_completed: MessageReader<'w, 's, ScenarioUiSelectionCompleted>,
    timed_completed: MessageWriter<'w, TimedScenarioUiActionCompleted>,
    authored: Query<'w, 's, (&'static UiDocumentOwner, &'static UiPresentationActions)>,
}

pub(super) fn apply_scenario_ui_commands(
    mut requests: MessageReader<ScenarioUiCommand>,
    mut state: Query<&mut ScenarioUiState>,
    worlds: Query<Entity, With<WorldRoot>>,
    mut events: ScenarioUiProjectionEvents,
    mut roles: MessageWriter<ShowUiRole>,
    owners: Query<Entity, With<InGameUiOwner>>,
    roots: Query<(Entity, &UiDocumentRoot)>,
    added_nodes: Query<(&UiNodeId, &UiDocumentOwner), Added<UiNodeId>>,
    documents: Res<Assets<UiDocumentAsset>>,
    mut presentation: ParamSet<(
        MessageReader<RequestUiPresentationAction>,
        MessageWriter<RequestUiPresentationAction>,
    )>,
    (mut actions, mut construction, mut subjects): (
        MessageWriter<ActionRequest>,
        MessageWriter<ScenarioConstructionRequest>,
        MessageWriter<SelectionRequest>,
    ),
    (mut sounds, mut disabled_catalogue, mut research_durations, mut unlocks): (
        MessageWriter<PlayAudioCue>,
        Query<&mut ScenarioDisabledCatalogueDefinitions, With<WorldRoot>>,
        MessageWriter<openzt2_zoo_state::progression::research_types::SetResearchDuration>,
        MessageWriter<
            openzt2_zoo_state::progression::unlock_types::ApplyCatalogueDefinitionUnlockRequest,
        >,
    ),
    mut animations: Query<(
        Entity,
        &UiNodeId,
        &UiDocumentOwner,
        &mut UiShowHideAnimation,
    )>,
    (all_nodes, members): (
        Query<(Entity, &UiNodeId, &UiDocumentOwner, &Visibility)>,
        Query<&UiDocumentMembers>,
    ),
    mut commands: Commands,
) {
    let Some(owner) = worlds
        .single()
        .ok()
        .or_else(|| owners.single().ok())
        .or_else(|| roots.iter().next().map(|(entity, _)| entity))
    else {
        for request in requests.read() {
            if let ScenarioUiCommand::Node {
                operation: NodeOperation::ScriptCompleted(action),
                ..
            } = request
            {
                events
                    .timed_completed
                    .write(TimedScenarioUiActionCompleted(*action));
            }
        }
        return;
    };
    let mut current = state.get_mut(owner).ok();
    let state_added = current.as_ref().is_some_and(|state| state.is_added());
    let mut initial = ScenarioUiState::default();
    let pending = current.as_deref_mut().unwrap_or(&mut initial);
    let selection_completed = events
        .selection_completed
        .read()
        .any(|completed| pending.selection.as_ref() == Some(completed));
    if selection_completed
        || pending
            .selection
            .is_some_and(|selection| !roots.contains(selection.document_root))
    {
        pending.selection = None;
    }

    let mut direct = presentation.p0();
    let advances = direct
        .read()
        .filter_map(|request| match request.action {
            UiPresentationAction::AdvanceTargetNodeMessageCursor { target_node } => {
                Some((request.document_root, target_node))
            }
            _ => None,
        })
        .chain(events.activations.read().flat_map(|event| {
            let (root, actions) = events.authored.get(event.node).ok().unzip();
            let root_entity = root.map(|root| root.0);
            let source = root_entity
                .and_then(|entity| roots.get(entity).ok())
                .and_then(|(_, root)| documents.get(&root.document));
            actions
                .zip(source)
                .into_iter()
                .flat_map(move |(actions, asset)| {
                    actions
                        .authored_action_records(asset)
                        .filter_map(move |record| match record.action {
                            UiPresentationAction::AdvanceTargetNodeMessageCursor {
                                target_node,
                            } if record.trigger == event.trigger => {
                                root_entity.map(|entity| (entity, target_node))
                            }
                            _ => None,
                        })
                })
        }))
        .collect::<Vec<_>>();
    let mut presentation = presentation.p1();
    for advance in advances {
        if let Some(panel) = pending.cached.get_mut(&advance) {
            if let Some(entry) = panel.entries.pop_front() {
                panel.visible = true;
                let _ = presentation.write_batch(entry.into_iter().map(|action| {
                    RequestUiPresentationAction {
                        document_root: advance.0,
                        action,
                    }
                }));
            } else {
                panel.visible = false;
            }
        }
    }

    let projected = !added_nodes.is_empty();
    let assets_changed = events.document_assets.read().count() > 0;
    if projected || assets_changed {
        pending.nodes.append(&mut pending.hidden);
    }
    let mut changed_enabled = Vec::new();
    for request in requests.read() {
        match request {
            ScenarioUiCommand::Node { path, operation } => {
                pending.nodes.push_back((path.clone(), operation.clone()))
            }
            ScenarioUiCommand::Tool(tool) => pending.nodes.push_back((
                Vec::new(),
                NodeOperation::Construction(ScenarioConstructionRequest::Select(*tool)),
            )),
            ScenarioUiCommand::PlacementObject(definition) => pending.nodes.push_back((
                Vec::new(),
                NodeOperation::Construction(ScenarioConstructionRequest::Place(*definition)),
            )),
            ScenarioUiCommand::FencePlacementMode(mode) => pending.nodes.push_back((
                Vec::new(),
                NodeOperation::Construction(ScenarioConstructionRequest::Mode(*mode)),
            )),
            ScenarioUiCommand::Select(entity) => pending
                .nodes
                .push_back((Vec::new(), NodeOperation::SelectSubject(*entity))),
            ScenarioUiCommand::Enabled { name, enabled } => {
                let key = name.to_ascii_lowercase();
                for (_, asset) in documents.iter() {
                    let document = asset.canonical_ui_document();
                    for (index, node) in document.nodes.iter().enumerate() {
                        if node.name.eq_ignore_ascii_case(name) {
                            continue;
                        }
                        if ancestors(document, index)
                            .any(|ancestor| ancestor.name.eq_ignore_ascii_case(name))
                        {
                            pending.enabled.remove(&node.name.to_ascii_lowercase());
                        }
                    }
                }
                pending.enabled.insert(key.clone(), *enabled);
                changed_enabled.push(key);
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
                    root: owner,
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
                pending.continuation = name
                    .as_deref()
                    .filter(|name| !name.is_empty())
                    .map(AssetId::from_key);
            }
            ScenarioUiCommand::Overhead => {
                actions.write(ActionRequest {
                    action: GameAction::OverheadView,
                    source: ActionSource::System,
                });
            }
            _ => {}
        }
    }
    if !changed_enabled.is_empty() || projected || state_added {
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
                // A buy-panel row enabled by name is no node of the document, so the
                // list that holds it stays usable inside a disabled group.
                let holds_enabled_row = pending.enabled.iter().any(|(name, enabled)| {
                    *enabled
                        && !document
                            .nodes
                            .iter()
                            .any(|node| node.name.eq_ignore_ascii_case(name))
                }) && std::iter::once(node)
                    .chain(ancestors(document, index))
                    .any(|node| node.name.eq_ignore_ascii_case("buy area"));
                if let Some((_, enabled)) = restriction {
                    if !changed_enabled.is_empty() || state_added || added() {
                        presentation.write(RequestUiPresentationAction {
                            document_root: root_entity,
                            action: UiPresentationAction::SetTargetNodeInteractionEnabled {
                                target_node: node.id,
                                enabled: enabled || holds_enabled_row,
                            },
                        });
                    }
                }
                // Pointer input stops at a disabled ancestor, and a child enabled
                // on its own has to work inside its disabled group. Its siblings
                // stay disabled through their own flags.
                if holds_enabled_row
                    || pending.enabled.get(&key) == Some(&true) && changed_enabled.contains(&key)
                {
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
                    if state_added || added() {
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
    if !changed_enabled.is_empty() || state_added {
        if let Ok(mut disabled) = disabled_catalogue.get_mut(owner) {
            let definitions = pending
                .enabled
                .iter()
                .filter(|(_, enabled)| !**enabled)
                .map(|(name, _)| AssetId::from_key(name))
                .collect();
            disabled.set_if_neq(ScenarioDisabledCatalogueDefinitions(definitions));
        }
    }
    let count = pending.nodes.len();
    for _ in 0..count {
        if pending.selection.is_some() {
            break;
        }
        let Some((path, operation)) = pending.nodes.pop_front() else {
            break;
        };
        if matches!(operation, NodeOperation::SelectionMode) {
            construction.write(ScenarioConstructionRequest::ReturnToSelection);
            continue;
        }
        if let NodeOperation::Construction(request) = operation {
            construction.write(request);
            continue;
        }
        if let NodeOperation::SelectSubject(entity) = operation {
            subjects.write(SelectionRequest {
                entity,
                source: ActionSource::System,
            });
            continue;
        }
        if matches!(operation, NodeOperation::MeasureText) {
            continue;
        }
        if let NodeOperation::ScriptCompleted(action) = operation {
            events
                .timed_completed
                .write(TimedScenarioUiActionCompleted(action));
            continue;
        }
        let target = roots.iter().find_map(|(entity, root)| {
            let document = documents.get(&root.document)?.canonical_ui_document();
            let id = resolve_node(document, &path)?;
            let ready = members.get(entity).is_ok_and(|members| {
                all_nodes
                    .iter_many(members.iter())
                    .flatten()
                    .any(|(_, node, ..)| node.id == id)
            });
            ready.then_some((entity, id))
        });
        let Some((root, target_node)) = target else {
            if let Some(role) = role_for(&documents, &path) {
                if (matches!(operation, NodeOperation::Visible(true))
                    && !(role == UiDocumentRole::PhotoMode && path.len() > 1))
                    || role == UiDocumentRole::Tutorial
                {
                    if let Ok(owner) = owners.single() {
                        roles.write(ShowUiRole { role, owner });
                    }
                }
            }
            pending.hidden.push_back((path, operation));
            continue;
        };
        let action = match operation {
            NodeOperation::Visible(visible) => UiPresentationAction::SetTargetNodeVisible {
                target_node,
                visible,
            },
            NodeOperation::Selected(active) => {
                let selection = ScenarioUiSelectionCompleted {
                    document_root: root,
                    target_node,
                };
                pending.selection = Some(selection);
                commands
                    .entity(root)
                    .insert(PendingScenarioUiSelection(selection));
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
            NodeOperation::RefreshGoals
            | NodeOperation::SelectionMode
            | NodeOperation::Construction(_)
            | NodeOperation::SelectSubject(_) => continue,
            NodeOperation::MeasureText | NodeOperation::ScriptCompleted(_) => continue,
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
                            .find(|(_, node, owner, _)| owner.0 == root && node.id == id)
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
        let document = documents
            .get(&roots.get(root).expect("resolved document").1.document)
            .expect("resolved document asset")
            .canonical_ui_document();
        let panel = document.nodes.iter().position(|node| node.id == target_node)
            .and_then(|index| std::iter::once(&document.nodes[index]).chain(ancestors(document, index)).find(|node| {
                node.cacheable_from_lua || node.actions.iter().any(|record| matches!(
                    record,
                    openzt2_game_data::ui_document::action::UiActionRecord::Presentation(record)
                        if matches!(record.action, UiPresentationAction::AdvanceTargetNodeMessageCursor { target_node } if target_node == node.id)
                ))
            }));
        if let Some(panel) = panel {
            let cache = pending.cached.entry((root, panel.id)).or_insert_with(|| {
                let visible = members.get(root).is_ok_and(|members| {
                    all_nodes
                        .iter_many(members.iter())
                        .flatten()
                        .any(|(_, node, _, visibility)| {
                            node.id == panel.id && *visibility != Visibility::Hidden
                        })
                });
                CachedPanel {
                    visible,
                    ..Default::default()
                }
            });
            let closes_panel = matches!(action, UiPresentationAction::SetTargetNodeVisible { target_node, visible: false } if target_node == panel.id);
            let opens_panel = matches!(action, UiPresentationAction::SetTargetNodeVisible { target_node, visible: true } if target_node == panel.id);
            let prepares_panel = matches!(
                action,
                UiPresentationAction::SetTargetNodeTextFromLocalizationKey { .. }
                    | UiPresentationAction::SetTargetNodeTextLiteral { .. }
                    | UiPresentationAction::SetTargetNodeModal { .. }
                    | UiPresentationAction::SetTargetNodeVisible { .. }
            );
            if closes_panel {
                cache.visible = false;
            } else if opens_panel {
                cache.draft.push(action);
                let entry = std::mem::take(&mut cache.draft);
                if cache.visible {
                    cache.entries.push_back(entry);
                } else {
                    cache.visible = true;
                    let _ = presentation.write_batch(entry.into_iter().map(|action| {
                        RequestUiPresentationAction {
                            document_root: root,
                            action,
                        }
                    }));
                }
                continue;
            } else if prepares_panel {
                cache.draft.push(action);
                continue;
            }
        } else if let UiPresentationAction::SetTargetNodeVisible { visible, .. } = action {
            if let [name] = path.as_slice() {
                pending.shown.insert(name.to_ascii_lowercase(), visible);
            }
        }
        presentation.write(RequestUiPresentationAction {
            document_root: root,
            action,
        });
    }
    if current.is_none()
        && (!initial.nodes.is_empty()
            || !initial.hidden.is_empty()
            || !initial.enabled.is_empty()
            || !initial.shown.is_empty()
            || initial.continuation.is_some()
            || initial.selection.is_some()
            || !initial.cached.is_empty())
    {
        commands.entity(owner).insert(initial);
    }
}

pub(super) fn apply_scenario_tool_requests(
    mut requests: MessageReader<ScenarioConstructionRequest>,
    mut tools: MessageWriter<SelectConstructionTool>,
    mut cancel: MessageWriter<CancelConstruction>,
    current_tool: Res<ConstructionTool>,
    mut policy: ResMut<ConstructionPlacementPolicy>,
    mut state: Query<&mut ScenarioUiState, With<WorldRoot>>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
) {
    let definitions = definitions.get(&assets);
    let mut state = state.single_mut().ok();
    let mut mode = state.as_ref().and_then(|state| state.fence_placement_mode);
    let mut current = *current_tool;
    for request in requests.read() {
        match request {
            ScenarioConstructionRequest::Select(tool) => {
                current = *tool;
                tools.write(SelectConstructionTool(current));
            }
            ScenarioConstructionRequest::Place(definition) => {
                let tool = definitions.and_then(|view| {
                    if view.find_fence(*definition).is_some() {
                        let fence = if mode == Some(SetScenarioFencePlacementMode::Gate) {
                            view.find_gate_fence(*definition)
                                .map_or(*definition, |gate| gate.id)
                        } else {
                            *definition
                        };
                        Some(ConstructionTool::Fence(fence))
                    } else if view.find_path(*definition).is_some() {
                        Some(ConstructionTool::Path(*definition))
                    } else if view.find_placeable(*definition).is_some() {
                        Some(ConstructionTool::Place(*definition))
                    } else {
                        None
                    }
                });
                if let Some(tool) = tool {
                    current = tool;
                    tools.write(SelectConstructionTool(current));
                } else {
                    warn!(
                        ?definition,
                        "scenario selected a placement type absent from definitions"
                    );
                }
            }
            ScenarioConstructionRequest::Mode(next) => {
                mode = Some(*next);
                if let Some(state) = state.as_mut() {
                    state
                        .reborrow()
                        .map_unchanged(|state| &mut state.fence_placement_mode)
                        .set_if_neq(mode);
                }
                let shape = match next {
                    SetScenarioFencePlacementMode::RubberBand
                    | SetScenarioFencePlacementMode::Gate => FencePlacementShape::Freeform,
                    SetScenarioFencePlacementMode::Rectangle => FencePlacementShape::Rectangle,
                    SetScenarioFencePlacementMode::Octagon => FencePlacementShape::Octagon,
                };
                cancel.write(CancelConstruction);
                policy
                    .reborrow()
                    .map_unchanged(|policy| &mut policy.fence_shape)
                    .set_if_neq(shape);
                if *next == SetScenarioFencePlacementMode::Gate {
                    if let ConstructionTool::Fence(fence) = current {
                        if let Some(gate) = definitions.and_then(|view| view.find_gate_fence(fence))
                        {
                            current = ConstructionTool::Fence(gate.id);
                            tools.write(SelectConstructionTool(current));
                        }
                    }
                }
            }
            ScenarioConstructionRequest::ReturnToSelection => {
                cancel.write(CancelConstruction);
                current = ConstructionTool::Inspect;
                tools.write(SelectConstructionTool(current));
            }
        }
    }
}

pub(super) fn apply_scenario_tank_requests(
    mut requests: MessageReader<ScenarioUiCommand>,
    mut heights: MessageWriter<SetTankWallHeights>,
) {
    for request in requests.read() {
        if let ScenarioUiCommand::TankWallHeight(height) = request {
            heights.write(SetTankWallHeights(*height));
        }
    }
}

pub(super) fn apply_scenario_zoo_messages(
    mut requests: MessageReader<ScenarioUiCommand>,
    time: Res<Time<Real>>,
    mut messages: Query<&mut ZooMessages, With<WorldRoot>>,
) {
    for request in requests.read() {
        if let ScenarioUiCommand::ZooMessage {
            root: Some(root),
            text,
            timeout_seconds,
        } = request
        {
            let Ok(mut messages) = messages.get_mut(*root) else {
                continue;
            };
            messages.messages.push(ZooMessage {
                text: text.clone(),
                expires_at_seconds: time.elapsed_secs_f64() + timeout_seconds,
            });
        }
    }
}

pub(super) fn apply_scenario_tutorial_status(
    mut requests: MessageReader<ScenarioUiCommand>,
    mut roots: Query<(Entity, Option<&mut ScenarioTutorialRuleActive>), With<WorldRoot>>,
    mut commands: Commands,
) {
    let Some(active) = requests
        .read()
        .filter_map(|request| match request {
            ScenarioUiCommand::TutorialRuleActive(active) => Some(*active),
            _ => None,
        })
        .last()
    else {
        return;
    };
    if let Ok((root, existing)) = roots.single_mut() {
        if let Some(mut existing) = existing {
            existing.set_if_neq(ScenarioTutorialRuleActive(active));
        } else {
            commands
                .entity(root)
                .insert(ScenarioTutorialRuleActive(active));
        }
    }
}
