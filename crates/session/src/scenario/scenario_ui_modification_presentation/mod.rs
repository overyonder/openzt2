use bevy::prelude::*;
use openzt2_game_assets::{
    source_document::{
        path::AssetPath,
        source_document_semantic_name::source_document_names_are_semantically_equal,
    },
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    world_scenario::{
        authored_world_scenario_state::{AuthoredScenarioUiModification, AuthoredScenarioUiState},
        world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios,
        world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
    },
};
use openzt2_game_data::{ui_document::node::UiNodeFlags, AssetId};
use openzt2_interface_state::ui::{
    authored_ui_interaction_enabled_state::{UiInteractionEnabled, UiScenarioInteractionOverride},
    authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot, UiNodeId},
};
use openzt2_world_state::{
    game_session_types::WorldSessionMode,
    world_spawn::{
        selected_world_document_handles::SelectedWorldDocumentHandles,
        selected_world_identity::SelectedWorldIdentity,
    },
};
use openzt2_zoo_state::scenario::scenario_session_types::{
    ActiveScenarioSession, ScenarioDocument, ScenarioSessionState,
};

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) struct AppliedScenarioUiModification {
    scenario: AssetId,
    old: AuthoredScenarioUiState,
    new: AuthoredScenarioUiState,
}

pub(super) fn project_running_scenario_ui_modifications(
    mut commands: Commands,
    worlds: Query<(
        Ref<SelectedWorldIdentity>,
        Option<Ref<ActiveScenarioSession>>,
        Option<Ref<ScenarioDocument>>,
        Option<Ref<SelectedWorldDocumentHandles>>,
    )>,
    (mut removed, mut document_changes): (
        RemovedComponents<ActiveScenarioSession>,
        MessageReader<AssetEvent<WorldScenarioDocumentAsset>>,
    ),
    scenarios: Res<WorldScenarios>,
    scenario_assets: Res<Assets<WorldScenarioDocumentAsset>>,
    ui_assets: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    mut nodes: Query<(
        Entity,
        &UiDocumentOwner,
        Ref<UiNodeId>,
        Option<&mut AppliedScenarioUiModification>,
        &mut Visibility,
        Option<&mut UiInteractionEnabled>,
        Option<&mut UiScenarioInteractionOverride>,
    )>,
) {
    let removed = removed.read().count() != 0;
    let document_changed = document_changes.read().count() != 0;
    let world = worlds.single().ok();
    let world_changed = world
        .as_ref()
        .is_some_and(|(selection, session, document, handles)| {
            selection.is_changed()
                || session.as_ref().is_some_and(|session| session.is_changed())
                || document
                    .as_ref()
                    .is_some_and(|document| document.is_changed())
                || handles.as_ref().is_some_and(|handles| handles.is_changed())
        });
    let scenario = world.and_then(|(selection, session, document, handles)| {
        if let (Some(session), Some(document)) = (session, document) {
            if session.state == ScenarioSessionState::Running {
                return Some((session.definition, scenario_assets.get(&document.0)?));
            }
            return None;
        }
        if let Some(mode) = handles.and_then(|handles| {
            handles
                .mode
                .as_ref()
                .and_then(|mode| scenario_assets.get(mode))
        }) {
            if let Some(scenario) = mode
                .document
                .scenarios
                .iter()
                .find(|scenario| mode.authored.ui_modifications.contains_key(&scenario.id))
            {
                return Some((scenario.id, mode));
            }
        }
        if selection.mode == WorldSessionMode::Challenge {
            let definition = AssetId::from_key("challengegoals");
            let scenarios = scenarios.get(&scenario_assets)?;
            return Some((
                definition,
                scenario_assets.get(scenarios.handle_for_scenario(definition)?)?,
            ));
        }
        None
    });
    for (node, document, id, previous, mut visibility, mut authored_gate, override_gate) in
        &mut nodes
    {
        if !world_changed && !removed && !document_changed && !id.is_added() {
            continue;
        }
        let Some(root) = roots.get(document.0).ok() else {
            continue;
        };
        let Some(authored_document) = ui_assets
            .get(&root.document)
            .map(UiDocumentAsset::canonical_ui_document)
        else {
            continue;
        };
        let Some(authored_node) = authored_document.nodes.get(id.index as usize) else {
            continue;
        };
        let current = scenario.and_then(|(scenario, asset)| {
            let rules = asset.authored.ui_modifications.get(&scenario)?;
            let rule = find_modification(rules, root, authored_document.id, &authored_node.name)?;
            Some(AppliedScenarioUiModification {
                scenario,
                old: rule.old,
                new: rule.new,
            })
        });
        let previous_state = previous.as_deref().copied();
        if let Some(old) = previous_state.filter(|old| Some(*old) != current) {
            if old.new.visible.is_some() && current.is_none_or(|rule| rule.new.visible.is_none()) {
                let visible = old
                    .old
                    .visible
                    .unwrap_or(id.index == 0 || authored_node.flags.contains(UiNodeFlags::VISIBLE));
                visibility.set_if_neq(project_visibility(visible));
            }
            if old.new.active.is_some() && current.is_none_or(|rule| rule.new.active.is_none()) {
                if let (Some(active), Some(gate)) = (old.old.active, &mut authored_gate) {
                    gate.set_if_neq(UiInteractionEnabled(active));
                }
            }
        }
        if let Some(visible) = current.and_then(|rule| rule.new.visible) {
            visibility.set_if_neq(project_visibility(visible));
        }
        if let Some(active) = current.and_then(|rule| rule.new.active) {
            let next = UiScenarioInteractionOverride(active);
            if let Some(mut gate) = override_gate {
                gate.set_if_neq(next);
            } else {
                commands.entity(node).insert(next);
            }
        } else if previous_state.is_some_and(|old| old.new.active.is_some())
            && override_gate.is_some()
        {
            commands
                .entity(node)
                .remove::<UiScenarioInteractionOverride>();
        }
        match (previous, current) {
            (Some(mut previous), Some(current)) => {
                previous.set_if_neq(current);
            }
            (None, Some(current)) => {
                commands.entity(node).insert(current);
            }
            (Some(_), None) => {
                commands
                    .entity(node)
                    .remove::<AppliedScenarioUiModification>();
            }
            (None, None) => {}
        }
    }
}

fn find_modification<'a>(
    rules: &'a [AuthoredScenarioUiModification],
    root: &UiDocumentRoot,
    document: AssetId,
    name: &str,
) -> Option<&'a AuthoredScenarioUiModification> {
    let path = root.document.path().map(|path| {
        AssetId::from_virtual_path(&AssetPath::new(&path.path().to_string_lossy()).key())
    });
    rules.iter().rev().find(|rule| {
        rule.document
            .is_none_or(|scope| scope == document || Some(scope) == path)
            && source_document_names_are_semantically_equal(&rule.node, name)
    })
}

fn project_visibility(visible: bool) -> Visibility {
    if visible {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}
