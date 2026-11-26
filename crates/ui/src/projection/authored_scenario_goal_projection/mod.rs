use bevy::prelude::*;
use openzt2_game_assets::{
    localization::{
        loaded_localization_queries::LoadedLocalizationView,
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
};
use openzt2_game_data::{
    ui_document::widget::UiWidgetRecord, world_scenario::ScenarioObjectiveGroupRecord, AssetId,
};
use openzt2_interface_state::ui::{
    authored_reusable_list_and_table_runtime_types::{
        UiListPolicy, UiScenarioGoalGroup, UiScenarioGoalGroupDocument, UiScenarioGoalRow,
    },
    ui_document_lifecycle_contracts::ShowUiDocument,
};
use openzt2_zoo_state::scenario::{
    scenario_objective_types::{ScenarioObjective, ScenarioObjectives},
    scenario_session_types::{ActiveScenarioSession, ScenarioDocument},
};

use super::{
    authored_document_projection::project_document_on_entity_with_nodes,
    document_asset_readiness::UiDocumentAssetEvents,
};

#[derive(Component)]
pub(crate) struct ScenarioGoalTreeOwner;

struct GoalFragments<'a> {
    list: Entity,
    group: (&'a Handle<UiDocumentAsset>, &'a UiDocumentAsset, usize),
    row: (&'a Handle<UiDocumentAsset>, &'a UiDocumentAsset),
    localization: LoadedLocalizationView<'a>,
    groups: &'a [ScenarioObjectiveGroupRecord],
    objectives: bevy::platform::collections::HashMap<AssetId, Entity>,
}

impl GoalFragments<'_> {
    fn project(&self, commands: &mut Commands, parent: Entity, members: &[AssetId]) {
        for &member in members {
            if let Some(&objective) = self.objectives.get(&member) {
                let (row, _) = project_document_on_entity_with_nodes(
                    commands,
                    &ShowUiDocument {
                        document: self.row.0.clone(),
                        owner: parent,
                    },
                    self.row.1,
                    self.localization,
                    None,
                    false,
                    None,
                );
                commands.entity(row).insert(UiScenarioGoalRow {
                    list: self.list,
                    objective,
                });
            } else if let Some(group) = self
                .groups
                .iter()
                .find(|group| group.id == member && !group.members.is_empty())
            {
                let (root, nodes) = project_document_on_entity_with_nodes(
                    commands,
                    &ShowUiDocument {
                        document: self.group.0.clone(),
                        owner: parent,
                    },
                    self.group.1,
                    self.localization,
                    None,
                    false,
                    None,
                );
                commands.entity(root).insert(UiScenarioGoalGroup {
                    list: self.list,
                    group: member,
                });
                self.project(commands, nodes[self.group.2], &group.members);
            }
        }
    }
}

pub(crate) fn project_authored_scenario_goal_trees(
    mut commands: Commands,
    lists: Query<(
        Entity,
        Ref<UiListPolicy>,
        Ref<UiScenarioGoalGroupDocument>,
        Option<&Children>,
    )>,
    generated: Query<(), With<ScenarioGoalTreeOwner>>,
    active: Query<(
        Ref<ActiveScenarioSession>,
        Ref<ScenarioDocument>,
        Option<Ref<ScenarioObjectives>>,
    )>,
    objectives: Query<&ScenarioObjective>,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    documents: Res<Assets<UiDocumentAsset>>,
    mut asset_events: UiDocumentAssetEvents,
    locales: Res<Assets<LocalizationAsset>>,
    active_locale: Res<LocalizationPrecedenceIndex>,
    (mut removed_sessions, mut removed_objectives): (
        RemovedComponents<ActiveScenarioSession>,
        RemovedComponents<ScenarioObjectives>,
    ),
) {
    let changes = asset_events.collect_changes();
    let removed = removed_sessions.read().count() + removed_objectives.read().count() != 0;
    let active = active.single().ok();
    let Some(localization) = active_locale.borrow_loaded_localization_view(&locales) else {
        return;
    };
    for (list, policy, group_doc, children) in &lists {
        let Some(row_handle) = &policy.row_document else {
            continue;
        };
        let dirty = policy.is_changed()
            || group_doc.is_changed()
            || removed
            || changes.affects(group_doc.0.id())
            || changes.affects(row_handle.id())
            || scenarios.is_changed()
            || active.as_ref().is_some_and(|(session, document, members)| {
                session.is_added()
                    || document.is_changed()
                    || members.as_ref().is_some_and(DetectChanges::is_changed)
            });
        if !dirty {
            continue;
        }
        let (Some(group_asset), Some(row)) =
            (documents.get(&group_doc.0), documents.get(row_handle))
        else {
            continue;
        };
        let Some(group_list) = group_asset
            .canonical_ui_document()
            .nodes
            .iter()
            .position(|node| matches!(&node.widget, UiWidgetRecord::List { .. }))
        else {
            continue;
        };
        for child in children
            .into_iter()
            .flat_map(|children| children.iter())
            .filter(|child| generated.contains(*child))
        {
            commands.entity(child).try_despawn();
        }
        let Some((session, handle, Some(members))) = active.as_ref() else {
            continue;
        };
        let Some(scenario) = scenarios
            .get(&handle.0)
            .and_then(|asset| asset.document.find_scenario_record(session.definition))
        else {
            continue;
        };
        let objectives = members
            .iter()
            .filter_map(|entity| {
                let objective = objectives.get(entity).ok()?;
                let record = scenario.objectives.get(objective.record_index as usize)?;
                Some((record.id, entity))
            })
            .collect();
        let flat_members;
        let root_members = if let Some(root) = scenario.objective_groups.last() {
            root.members.as_slice()
        } else {
            flat_members = scenario
                .objectives
                .iter()
                .map(|objective| objective.id)
                .collect::<Vec<_>>();
            &flat_members
        };
        let parent = commands
            .spawn((
                ScenarioGoalTreeOwner,
                ChildOf(list),
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .id();
        GoalFragments {
            list,
            group: (&group_doc.0, group_asset, group_list),
            row: (row_handle, row),
            localization,
            objectives,
            groups: &scenario.objective_groups,
        }
        .project(&mut commands, parent, root_members);
    }
}
