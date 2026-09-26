use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::{
        node_property_binding::UiTextPropertyBindingSource,
        widget_live_collection::UiWidgetLiveCollectionSource,
    },
    AssetId,
};

use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::SetUiListRowCount;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding;
use crate::plugins::ui::scenario_objective_status_visual_classification::UiScenarioObjectiveStatusVisualClassification;

use super::{
    scenario_objective_types::{
        ScenarioObjective, ScenarioObjectiveAuthoredVisibility, ScenarioObjectiveStatus,
    },
    scenario_session_types::{ActiveScenarioSession, SelectedScenarioDocument},
    scenario_ui_types::{
        ScenarioGoalListProjection, ScenarioGoalRowProjection, ScenarioObjectiveCategoryFilter,
        ScenarioObjectiveStatusFilter,
    },
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

/// Sizes authored goal panels from the active scenario objective range.
pub(super) fn set_authored_scenario_goal_panel_row_counts(
    mut commands: Commands,
    source: Option<Res<SelectedScenarioDocument>>,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    active: Query<&ActiveScenarioSession>,
    mut lists: Query<(
        Entity,
        &UiListPolicy,
        &UiDocumentOwner,
        Option<&mut ScenarioGoalListProjection>,
    )>,
    category_filters: Query<&ScenarioObjectiveCategoryFilter>,
    mut row_counts: MessageWriter<SetUiListRowCount>,
) {
    let active_scenario_and_objective_count = source
        .as_deref()
        .and_then(|source| scenarios.get(&source.0))
        .and_then(|asset| {
            active
                .single()
                .ok()
                .and_then(|active| asset.document.find_scenario_record(active.definition))
                .map(|scenario| {
                    (
                        AssetId(scenario.id.0),
                        u16::try_from(scenario.objectives.len()).unwrap_or(u16::MAX),
                    )
                })
        });

    for (entity, policy, owner, current_projection) in &mut lists {
        if policy.source != UiWidgetLiveCollectionSource::ScenarioObjectives {
            continue;
        }
        if category_filters.get(owner.0).is_err() {
            commands
                .entity(owner.0)
                .insert(ScenarioObjectiveCategoryFilter(AssetId::from_key(
                    "overview",
                )));
        }
        let (scenario, projected_rows) =
            active_scenario_and_objective_count.unwrap_or((AssetId::default(), 0));
        let projection_changed = current_projection
            .as_ref()
            .is_none_or(|current_projection| {
                current_projection.scenario != scenario
                    || current_projection.projected_rows != projected_rows
            });
        if !projection_changed {
            continue;
        }
        commands.entity(entity).insert(ScenarioGoalListProjection {
            scenario,
            projected_rows,
        });
        row_counts.write(SetUiListRowCount {
            list: entity,
            count: projected_rows,
        });
    }
}

/// Connects each authored row to its canonical objective and status visual.
#[allow(clippy::too_many_arguments)]
pub(super) fn connect_authored_scenario_goal_rows_to_live_objectives(
    mut commands: Commands,
    source: Option<Res<SelectedScenarioDocument>>,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    lists: Query<&ScenarioGoalListProjection>,
    rows: Query<(Entity, &UiListRow, Option<&ScenarioGoalRowProjection>)>,
    objectives: Query<(Entity, &ScenarioObjective, &ScenarioObjectiveStatus)>,
    parents: Query<&ChildOf>,
    texts: Query<(Entity, Option<&UiTextBinding>), With<Text>>,
    mut status_nodes: Query<
        (
            Entity,
            &UiScenarioObjectiveStatusVisualClassification,
            &mut Visibility,
        ),
        Without<UiListRow>,
    >,
) {
    let Some(asset) = source
        .as_deref()
        .and_then(|source| scenarios.get(&source.0))
    else {
        return;
    };
    for (row_entity, row, current_projection) in &rows {
        let Ok(list) = lists.get(row.list) else {
            continue;
        };
        let Some(scenario) = asset.document.find_scenario_record(list.scenario) else {
            continue;
        };
        let record_index = u32::from(row.index);
        let Some(record) = scenario.objectives.get(record_index as usize) else {
            continue;
        };
        let Some((objective_entity, _, status)) = objectives.iter().find(|(_, objective, _)| {
            objective.scenario == list.scenario && objective.record_index == record_index
        }) else {
            continue;
        };
        let projection = ScenarioGoalRowProjection {
            objective: objective_entity,
            definition: AssetId(record.id.0),
        };
        if current_projection != Some(&projection) {
            commands.entity(row_entity).insert(projection);
        }
        let text_binding = UiTextBinding(UiTextPropertyBindingSource::ScenarioObjectiveText {
            objective: projection.definition,
        });
        for (entity, current_binding) in &texts {
            if entity_has_ancestor(entity, row_entity, &parents)
                && current_binding != Some(&text_binding)
            {
                commands.entity(entity).insert(text_binding.clone());
            }
        }
        for (entity, visual, mut visibility) in &mut status_nodes {
            if !entity_has_ancestor(entity, row_entity, &parents) {
                continue;
            }
            let expected_status = ScenarioObjectiveStatusFilter::from(&visual.0);
            *visibility = if expected_status.includes_objective_status(*status) {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}

/// Applies objective category and status filters without copying objective data.
pub(super) fn apply_scenario_goal_panel_filters_to_projected_rows(
    source: Option<Res<SelectedScenarioDocument>>,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    status_filters: Query<&ScenarioObjectiveStatusFilter>,
    category_filters: Query<&ScenarioObjectiveCategoryFilter>,
    lists: Query<(&ScenarioGoalListProjection, &UiDocumentOwner)>,
    mut rows: Query<(&UiListRow, &ScenarioGoalRowProjection, &mut Visibility)>,
    mut descriptions: Query<
        (&UiDocumentOwner, &UiTextBinding, &mut Visibility),
        Without<UiListRow>,
    >,
    objectives: Query<(
        &ScenarioObjective,
        &ScenarioObjectiveStatus,
        &ScenarioObjectiveAuthoredVisibility,
    )>,
) {
    let scenario_document = source
        .as_deref()
        .and_then(|source| scenarios.get(&source.0))
        .map(|asset| &asset.document);
    for (row, projection, mut visibility) in &mut rows {
        let Ok((_, owner)) = lists.get(row.list) else {
            continue;
        };
        let status_filter = status_filters.get(owner.0).copied().unwrap_or_default();
        let category_filter = category_filters.get(owner.0).ok().map(|filter| filter.0);
        let Ok((objective, status, authored_visible)) = objectives.get(projection.objective) else {
            continue;
        };
        let category_matches = category_filter.is_none_or(|filter| {
            filter == AssetId::default()
                || scenario_document
                    .and_then(|scenario_document| {
                        scenario_document
                            .find_scenario_objective(objective.scenario, objective.record_index)
                    })
                    .is_some_and(|record| record.category.0 == filter.0)
        });
        *visibility = if authored_visible.0
            && category_matches
            && status_filter.includes_objective_status(*status)
        {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    let overview_category = AssetId::from_key("overview");
    for (owner, binding, mut visibility) in &mut descriptions {
        if !matches!(&binding.0, UiTextPropertyBindingSource::ScenarioDescription) {
            continue;
        }
        *visibility = if category_filters
            .get(owner.0)
            .is_ok_and(|filter| filter.0 == overview_category)
        {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn entity_has_ancestor(entity: Entity, ancestor: Entity, parents: &Query<&ChildOf>) -> bool {
    std::iter::successors(Some(entity), |entity| {
        parents.get(*entity).ok().map(ChildOf::parent)
    })
    .skip(1)
    .any(|entity| entity == ancestor)
}
