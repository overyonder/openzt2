use bevy::prelude::*;
use openzt2_game_assets::{
    source_document::source_document_semantic_name::source_document_names_are_semantically_equal,
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    world_scenario::{
        world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios,
        world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
    },
};
use openzt2_game_data::{ui_document::node::UiNodeFlags, AssetId};
use openzt2_interface_state::ui::authored_ui_node_projection_components::{
    UiDocumentOwner, UiDocumentRoot, UiNodeId,
};
use openzt2_world_state::{
    game_session_types::WorldSessionMode,
    world_spawn::selected_world_identity::SelectedWorldIdentity,
};
use openzt2_zoo_state::scenario::scenario_session_types::{
    ActiveScenarioSession, ScenarioDocument, ScenarioSessionState,
};

#[derive(Component)]
pub(super) struct ScenarioUiVisibilityOverride;

pub(super) fn project_running_scenario_ui_modifications(
    mut commands: Commands,
    worlds: Query<(
        Ref<SelectedWorldIdentity>,
        Option<Ref<ActiveScenarioSession>>,
        Option<Ref<ScenarioDocument>>,
    )>,
    mut removed: RemovedComponents<ActiveScenarioSession>,
    mut document_changes: MessageReader<AssetEvent<WorldScenarioDocumentAsset>>,
    scenarios: Res<WorldScenarios>,
    scenario_assets: Res<Assets<WorldScenarioDocumentAsset>>,
    ui_assets: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    mut nodes: Query<(
        Entity,
        &UiDocumentOwner,
        Ref<UiNodeId>,
        Has<ScenarioUiVisibilityOverride>,
        &mut Visibility,
    )>,
) {
    let removed = removed.read().count() != 0;
    let document_changed = document_changes.read().count() != 0;
    let world = worlds.single().ok();
    let world_changed = world
        .as_ref()
        .is_some_and(|(selection, session, document)| {
            selection.is_changed()
                || session.as_ref().is_some_and(|session| session.is_changed())
                || document
                    .as_ref()
                    .is_some_and(|document| document.is_changed())
        });
    let scenario = world.and_then(|(selection, session, document)| {
        if let (Some(session), Some(document)) = (session, document) {
            if session.state == ScenarioSessionState::Running {
                return scenario_assets
                    .get(&document.0)?
                    .document
                    .find_scenario_record(session.definition);
            }
            return None;
        }
        if selection.mode == WorldSessionMode::Challenge {
            return scenarios
                .get(&scenario_assets)?
                .scenario(AssetId::from_key("challengegoals"));
        }
        None
    });
    for (node, document, id, overridden, mut visibility) in &mut nodes {
        if !world_changed && !removed && !document_changed && !id.is_added() {
            continue;
        }
        let Some(authored) = roots
            .get(document.0)
            .ok()
            .and_then(|root| ui_assets.get(&root.document))
            .and_then(|asset| asset.canonical_ui_document().nodes.get(id.index as usize))
        else {
            continue;
        };
        let rule = scenario.and_then(|scenario| {
            scenario.ui_modifications.iter().find(|rule| {
                source_document_names_are_semantically_equal(&authored.name, &rule.node)
            })
        });
        let visible = if let Some(rule) = rule {
            if !overridden {
                commands.entity(node).insert(ScenarioUiVisibilityOverride);
            }
            rule.visible
        } else if overridden {
            commands
                .entity(node)
                .remove::<ScenarioUiVisibilityOverride>();
            id.index == 0 || authored.flags.contains(UiNodeFlags::VISIBLE)
        } else {
            continue;
        };
        visibility.set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}
