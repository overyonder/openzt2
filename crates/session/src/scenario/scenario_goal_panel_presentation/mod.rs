use bevy::{
    ecs::{entity::EntityHashSet, relationship::RelationshipTarget},
    prelude::*,
};
use openzt2_game_data::{
    ui_document::{
        node_property_binding::UiTextPropertyBindingSource,
        widget_live_collection::UiWidgetLiveCollectionSource,
    },
    AssetId,
};

use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::SetUiListRowCount;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use openzt2_interface_state::ui::authored_ui_node_projection_components::{
    UiDocumentMembers, UiDocumentOwner,
};
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;
use openzt2_ui::scenario_objective_status_visual_classification::UiScenarioObjectiveStatusVisualClassification;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;

use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjective;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveAuthoredVisibility;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatus;

use super::scenario_ui_types::{
    ScenarioGoalRowOf, ScenarioGoalRows, ScenarioObjectiveCategoryFilter,
    ScenarioObjectiveStatusFilter,
};

impl From<&openzt2_game_data::ui_document::action::scenarios::ObjectiveStatusFilter>
    for ScenarioObjectiveStatusFilter
{
    fn from(
        value: &openzt2_game_data::ui_document::action::scenarios::ObjectiveStatusFilter,
    ) -> Self {
        use openzt2_game_data::ui_document::action::scenarios::ObjectiveStatusFilter;

        match value {
            ObjectiveStatusFilter::All => Self::All,
            ObjectiveStatusFilter::Success => Self::Success,
            ObjectiveStatusFilter::Failure => Self::Failure,
            ObjectiveStatusFilter::Neutral => Self::Neutral,
        }
    }
}

impl ScenarioObjectiveStatusFilter {
    fn includes_objective_status(self, status: ScenarioObjectiveStatus) -> bool {
        match self {
            Self::All => true,
            Self::Success => status == ScenarioObjectiveStatus::Satisfied,
            Self::Failure => status == ScenarioObjectiveStatus::Failed,
            Self::Neutral => matches!(
                status,
                ScenarioObjectiveStatus::Inactive | ScenarioObjectiveStatus::Active
            ),
        }
    }
}

pub(super) fn size_and_bind_authored_scenario_goal_rows(
    mut commands: Commands,
    objectives: Query<(Entity, &PersistentId), With<ScenarioObjective>>,
    changed_objectives: Query<
        (),
        (
            With<ScenarioObjective>,
            Or<(Added<ScenarioObjective>, Changed<PersistentId>)>,
        ),
    >,
    mut removed_objectives: RemovedComponents<ScenarioObjective>,
    lists: Query<(
        Entity,
        Ref<UiListPolicy>,
        &UiDocumentOwner,
        Option<&Children>,
    )>,
    rows: Query<(Entity, &UiListRow, &ChildOf, Option<&ScenarioGoalRowOf>)>,
    changed_rows: Query<Entity, (With<UiListRow>, Or<(Changed<UiListRow>, Changed<ChildOf>)>)>,
    category_filters: Query<&ScenarioObjectiveCategoryFilter>,
    mut row_counts: MessageWriter<SetUiListRowCount>,
) {
    let membership_changed =
        removed_objectives.read().count() != 0 || !changed_objectives.is_empty();
    let new_lists = lists.iter().any(|(_, policy, _, _)| {
        policy.source == UiWidgetLiveCollectionSource::ScenarioObjectives && policy.is_changed()
    });
    if !(membership_changed || new_lists || !changed_rows.is_empty()) {
        return;
    }
    let mut resize = EntityHashSet::default();
    let mut affected = EntityHashSet::default();
    for (list, policy, owner, children) in &lists {
        if policy.source != UiWidgetLiveCollectionSource::ScenarioObjectives {
            continue;
        }
        if membership_changed || policy.is_changed() {
            if category_filters.get(owner.0).is_err() {
                commands
                    .entity(owner.0)
                    .insert(ScenarioObjectiveCategoryFilter(AssetId::from_key(
                        "overview",
                    )));
            }
            resize.insert(list);
            affected.extend(children.into_iter().flatten().copied());
        }
    }
    affected.extend(changed_rows.iter().filter(|row| {
        rows.get(*row)
            .ok()
            .and_then(|(_, _, parent, _)| lists.get(parent.parent()).ok())
            .is_some_and(|(_, policy, _, _)| {
                policy.source == UiWidgetLiveCollectionSource::ScenarioObjectives
            })
    }));
    if affected.is_empty() && resize.is_empty() {
        return;
    }

    let mut ordered: Vec<_> = objectives.iter().collect();
    ordered.sort_unstable_by_key(|(_, id)| id.0);
    for list in resize {
        row_counts.write(SetUiListRowCount {
            list,
            count: u16::try_from(ordered.len()).unwrap_or(u16::MAX),
        });
    }
    for row in affected {
        let Ok((entity, row, _, bound)) = rows.get(row) else {
            continue;
        };
        match ordered.get(usize::from(row.index)) {
            Some(&(objective, _)) if bound.is_none_or(|bound| bound.0 != objective) => {
                commands.entity(entity).insert(ScenarioGoalRowOf(objective));
            }
            None if bound.is_some() => {
                commands.entity(entity).remove::<ScenarioGoalRowOf>();
            }
            _ => {}
        }
    }
}

pub(super) fn project_changed_scenario_goal_rows(
    mut commands: Commands,
    scenarios: Res<WorldScenarios>,
    scenario_assets: Res<Assets<WorldScenarioDocumentAsset>>,
    status_filters: Query<Ref<ScenarioObjectiveStatusFilter>>,
    category_filters: Query<Ref<ScenarioObjectiveCategoryFilter>>,
    lists: Query<&UiDocumentOwner, With<UiListPolicy>>,
    documents: Query<&UiDocumentMembers>,
    changed_filters: Query<
        Entity,
        Or<(
            Changed<ScenarioObjectiveStatusFilter>,
            Changed<ScenarioObjectiveCategoryFilter>,
        )>,
    >,
    changed_objectives: Query<
        &ScenarioGoalRows,
        Or<(
            Changed<ScenarioObjectiveStatus>,
            Changed<ScenarioObjectiveAuthoredVisibility>,
            Changed<ScenarioObjective>,
        )>,
    >,
    changed_rows: Query<Entity, (With<UiListRow>, Changed<ScenarioGoalRowOf>)>,
    added_children: Query<
        Entity,
        Or<(
            Added<Text>,
            Added<UiScenarioObjectiveStatusVisualClassification>,
            Changed<ChildOf>,
        )>,
    >,
    mut rows: Query<(Entity, &ChildOf, &ScenarioGoalRowOf, &mut Visibility), With<UiListRow>>,
    objectives: Query<(
        &ScenarioObjective,
        &ScenarioObjectiveStatus,
        &ScenarioObjectiveAuthoredVisibility,
    )>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    mut texts: Query<Option<&mut UiTextBinding>, With<Text>>,
    mut status_nodes: Query<
        (
            &UiScenarioObjectiveStatusVisualClassification,
            &mut Visibility,
            &mut Node,
        ),
        Without<UiListRow>,
    >,
) {
    let catalogue_changed = scenarios.is_changed() || scenario_assets.is_changed();
    let Some(scenarios) = scenarios.get(&scenario_assets) else {
        return;
    };
    let mut affected: EntityHashSet = changed_rows.iter().collect();
    for related in &changed_objectives {
        affected.extend(related.iter());
    }
    for document in &changed_filters {
        if let Ok(members) = documents.get(document) {
            affected.extend(members.iter().filter(|member| rows.contains(*member)));
        }
    }
    for child in &added_children {
        if let Some(row) = parents
            .iter_ancestors::<ChildOf>(child)
            .find(|ancestor| rows.contains(*ancestor))
        {
            affected.insert(row);
        }
    }
    if catalogue_changed {
        affected.extend(rows.iter().map(|(entity, ..)| entity));
    }
    for row in affected {
        let Ok((entity, parent, bound, mut visibility)) = rows.get_mut(row) else {
            continue;
        };
        let Ok(owner) = lists.get(parent.parent()) else {
            continue;
        };
        let Ok((objective, status, authored_visible)) = objectives.get(bound.0) else {
            continue;
        };
        let record = scenarios
            .scenario(objective.scenario)
            .and_then(|scenario| scenario.objectives.get(objective.record_index as usize));
        let category_matches = category_filters.get(owner.0).ok().is_none_or(|filter| {
            filter.0 == AssetId::default()
                || record.is_some_and(|record| record.category == filter.0)
        });
        let filter = status_filters
            .get(owner.0)
            .ok()
            .as_deref()
            .copied()
            .unwrap_or_default();
        visibility.set_if_neq(
            if authored_visible.0 && category_matches && filter.includes_objective_status(*status) {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
        );
        for child in children.iter_descendants::<Children>(entity) {
            if let (Some(record), Ok(binding)) = (record, texts.get_mut(child)) {
                let binding_value =
                    UiTextBinding(UiTextPropertyBindingSource::ScenarioObjectiveText {
                        objective: record.id,
                    });
                if let Some(mut binding) = binding {
                    binding.set_if_neq(binding_value);
                } else {
                    commands.entity(child).insert(binding_value);
                }
            }
            if let Ok((visual, mut visibility, mut node)) = status_nodes.get_mut(child) {
                let filter = ScenarioObjectiveStatusFilter::from(&visual.0);
                visibility.set_if_neq(if filter.includes_objective_status(*status) {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
                let columns = [RepeatedGridTrack::auto(1), RepeatedGridTrack::fr(1, 1.0)];
                if node.grid_template_columns.as_slice() != columns.as_slice() {
                    node.grid_template_columns = columns.to_vec();
                }
                if node.justify_items != JustifyItems::Stretch {
                    node.justify_items = JustifyItems::Stretch;
                }
            }
        }
    }
}

pub(super) fn project_scenario_description_visibility(
    filters: Query<&ScenarioObjectiveCategoryFilter>,
    changed_filters: Query<Entity, Changed<ScenarioObjectiveCategoryFilter>>,
    mut removed_filters: RemovedComponents<ScenarioObjectiveCategoryFilter>,
    documents: Query<&UiDocumentMembers>,
    projected_text: Query<Entity, Or<(Changed<UiTextBinding>, Changed<UiDocumentOwner>)>>,
    mut descriptions: Query<
        (&UiDocumentOwner, &UiTextBinding, &mut Visibility),
        Without<UiListRow>,
    >,
) {
    let overview = AssetId::from_key("overview");
    let mut affected: EntityHashSet = projected_text.iter().collect();
    for document in changed_filters.iter().chain(removed_filters.read()) {
        if let Ok(members) = documents.get(document) {
            affected.extend(members.iter());
        }
    }
    for entity in affected {
        let Ok((owner, binding, mut visibility)) = descriptions.get_mut(entity) else {
            continue;
        };
        if binding.0 != UiTextPropertyBindingSource::ScenarioDescription {
            continue;
        }
        visibility.set_if_neq(
            if filters
                .get(owner.0)
                .is_ok_and(|filter| filter.0 == overview)
            {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
        );
    }
}
