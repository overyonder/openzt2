use super::{
    challenge_offer_types::{ScenarioChallengeOffer, ScenarioChallengeOfferState},
    scenario_lua_ui::presentation::{DirectScenarioUiInteraction, NamedScenarioUiInteraction},
};
use bevy::prelude::*;
use openzt2_game_assets::{
    source_document::{
        path::AssetPath,
        source_document_semantic_name::source_document_names_are_semantically_equal,
    },
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    world_scenario::{
        authored_world_scenario_state::{AuthoredScenarioUiModification, AuthoredScenarioUiState},
        world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
    },
};
use openzt2_game_data::{
    ui_document::{
        action::{scenarios::UiScenarioAction, shell_navigation::UiShellAction, UiActionRecord},
        document::UiDocumentRole,
        node::UiNodeFlags,
    },
    AssetId,
};
use openzt2_interface_state::ui::{
    authored_ui_interaction_enabled_state::{UiInteractionEnabled, UiScenarioInteractionOverride},
    authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot, UiNodeId},
};
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
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

impl AppliedScenarioUiModification {
    pub(super) fn active_override(&self) -> Option<bool> {
        self.new.active
    }
}

pub(super) fn project_running_scenario_ui_modifications(
    mut commands: Commands,
    worlds: Query<
        (
            Entity,
            Ref<SelectedWorldIdentity>,
            Option<Ref<ActiveScenarioSession>>,
            Option<Ref<ScenarioDocument>>,
            Option<Ref<SelectedWorldDocumentHandles>>,
        ),
        With<WorldRoot>,
    >,
    offers: Query<(
        &WorldMember,
        Ref<ScenarioChallengeOffer>,
        Ref<ScenarioDocument>,
    )>,
    mut retired_offers: RemovedComponents<ScenarioChallengeOffer>,
    (mut removed, mut document_changes): (
        RemovedComponents<ActiveScenarioSession>,
        MessageReader<AssetEvent<WorldScenarioDocumentAsset>>,
    ),
    scenario_assets: Res<Assets<WorldScenarioDocumentAsset>>,
    ui_assets: Res<Assets<UiDocumentAsset>>,
    mut ui_changes: MessageReader<AssetEvent<UiDocumentAsset>>,
    (catalogue, archives, server): (
        Res<openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios>,
        Res<openzt2_game_assets::asset_source::AssetArchives>,
        Res<AssetServer>,
    ),
    roots: Query<&UiDocumentRoot>,
    mut nodes: Query<(
        Entity,
        &UiDocumentOwner,
        Ref<UiNodeId>,
        Option<&mut AppliedScenarioUiModification>,
        &mut Visibility,
        Option<&mut UiInteractionEnabled>,
        Option<&mut UiScenarioInteractionOverride>,
        Has<NamedScenarioUiInteraction>,
        Has<DirectScenarioUiInteraction>,
    )>,
) {
    let removed = removed.read().count() != 0 || retired_offers.read().count() != 0;
    let document_changed = document_changes.read().count() + ui_changes.read().count() != 0;
    let world = worlds.single().ok();
    let mut sources = Vec::new();
    let mut world_changed = false;
    if let Some((root, selection, session, document, handles)) = world {
        world_changed = catalogue.is_changed()
            || selection.is_changed()
            || session.as_ref().is_some_and(|session| session.is_changed())
            || document
                .as_ref()
                .is_some_and(|document| document.is_changed())
            || handles.as_ref().is_some_and(|handles| handles.is_changed());
        if selection.mode == WorldSessionMode::Challenge {
            let selected = handles
                .as_ref()
                .and_then(|handles| handles.mode.as_ref())
                .and_then(|mode| scenario_assets.get(mode))
                .and_then(|asset| asset.manager().map(|manager| (asset, manager)));
            let manager = selected.or_else(|| {
                let path = authored_challenge_manager_path(&ui_assets)?;
                catalogue
                    .get(&scenario_assets)?
                    .manager_for_path(path, &archives, &server)
            });
            if let Some((asset, manager)) = manager {
                if let Some(rules) = asset.authored.ui_modifications.get(&manager.id) {
                    sources.push((manager.id, rules));
                }
            }
        }
        if let Some((session, document)) = session.zip(document) {
            if session.state == ScenarioSessionState::Running {
                if let Some(rules) = scenario_assets
                    .get(&document.0)
                    .and_then(|asset| asset.authored.ui_modifications.get(&session.definition))
                {
                    sources.push((session.definition, rules));
                }
            }
        }
        for (member, offer, document) in &offers {
            if member.root != root {
                continue;
            }
            world_changed |= offer.is_changed() || document.is_changed();
            if offer.state != ScenarioChallengeOfferState::Declined {
                if let Some(rules) = scenario_assets
                    .get(&document.0)
                    .and_then(|asset| asset.authored.ui_modifications.get(&offer.definition))
                {
                    sources.push((offer.definition, rules));
                }
            }
        }
    }
    for (
        node,
        document,
        id,
        previous,
        mut visibility,
        mut authored_gate,
        override_gate,
        named,
        direct,
    ) in &mut nodes
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
        let mut current: Option<AppliedScenarioUiModification> = None;
        for &(scenario, rules) in &sources {
            let Some(rule) =
                find_modification(rules, root, authored_document.id, &authored_node.name)
            else {
                continue;
            };
            let lower = current.get_or_insert(AppliedScenarioUiModification {
                scenario,
                old: AuthoredScenarioUiState::default(),
                new: AuthoredScenarioUiState::default(),
            });
            lower.scenario = scenario;
            lower.old.active = lower.old.active.or(rule.old.active);
            lower.old.visible = lower.old.visible.or(rule.old.visible);
            lower.new.active = rule.new.active.or(lower.new.active);
            lower.new.visible = rule.new.visible.or(lower.new.visible);
        }
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
        if !named && !direct {
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

fn authored_challenge_manager_path(documents: &Assets<UiDocumentAsset>) -> Option<&str> {
    let main = documents
        .iter()
        .map(|(_, asset)| asset.canonical_ui_document())
        .find(|document| document.role == UiDocumentRole::MainMenu)?;
    main.nodes.iter()
        .filter(|node| node.actions.iter().any(|action| matches!(action,
            UiActionRecord::Shell(action) if matches!(action.action, UiShellAction::SelectChallengeModeOrStartSelectedWorld)
        )))
        .flat_map(|node| &node.actions)
        .find_map(|action| match action {
            UiActionRecord::Scenario(action) => match &action.action {
                UiScenarioAction::SelectionChanged { scenario_path } => scenario_path.as_deref(),
                _ => None,
            },
            _ => None,
        })
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
