use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::{
        node_property_binding::UiTextPropertyBindingSource,
        widget_live_collection::UiWidgetLiveCollectionSource,
    },
    AssetId,
};

use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::SetUiListRowCount;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding;
use crate::plugins::ui::scenario_objective_status_visual_classification::UiScenarioObjectiveStatusVisualClassification;
use crate::plugins::world_spawn::persistent_id_types::PersistentId;
use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

use super::{
    scenario_objective_types::{
        ScenarioObjective, ScenarioObjectiveAuthoredVisibility, ScenarioObjectiveStatus,
    },
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

/// Sizes authored goal panels from the world's live objectives: the campaign
/// scenario's and those of every challenge, in the order they were created.
/// The original goal panel lists the rules of every scenario its manager holds.
pub(super) fn set_authored_scenario_goal_panel_row_counts(
    mut commands: Commands,
    objectives: Query<(Entity, &PersistentId), With<ScenarioObjective>>,
    mut lists: Query<(
        Entity,
        &UiListPolicy,
        &UiDocumentOwner,
        Option<&ScenarioGoalListProjection>,
    )>,
    category_filters: Query<&ScenarioObjectiveCategoryFilter>,
    mut row_counts: MessageWriter<SetUiListRowCount>,
) {
    let mut ordered = objectives.iter().collect::<Vec<_>>();
    ordered.sort_unstable_by_key(|(_, id)| id.0);
    let projected = ordered
        .into_iter()
        .map(|(entity, _)| entity)
        .collect::<Vec<_>>();
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
        if current_projection.is_some_and(|current| current.objectives == projected) {
            continue;
        }
        commands.entity(entity).insert(ScenarioGoalListProjection {
            objectives: projected.clone(),
        });
        row_counts.write(SetUiListRowCount {
            list: entity,
            count: u16::try_from(projected.len()).unwrap_or(u16::MAX),
        });
    }
}

/// Connects each authored row to its canonical objective and status visual.
#[allow(clippy::too_many_arguments)]
pub(super) fn connect_authored_scenario_goal_rows_to_live_objectives(
    mut commands: Commands,
    scenarios: Res<WorldScenarios>,
    scenario_assets: Res<Assets<WorldScenarioDocumentAsset>>,
    lists: Query<&ScenarioGoalListProjection>,
    rows: Query<(Entity, &UiListRow, Option<&ScenarioGoalRowProjection>)>,
    objectives: Query<(&ScenarioObjective, &ScenarioObjectiveStatus)>,
    parents: Query<&ChildOf>,
    texts: Query<(Entity, Option<&UiTextBinding>), With<Text>>,
    mut status_nodes: Query<
        (
            Entity,
            &UiScenarioObjectiveStatusVisualClassification,
            &mut Visibility,
            &mut Node,
        ),
        Without<UiListRow>,
    >,
) {
    let Some(scenarios) = scenarios.get(&scenario_assets) else {
        return;
    };
    for (row_entity, row, current_projection) in &rows {
        let Ok(list) = lists.get(row.list) else {
            continue;
        };
        let Some(&objective_entity) = list.objectives.get(usize::from(row.index)) else {
            continue;
        };
        let Ok((objective, status)) = objectives.get(objective_entity) else {
            continue;
        };
        let Some(record) = scenarios
            .scenario(objective.scenario)
            .and_then(|scenario| scenario.objectives.get(objective.record_index as usize))
        else {
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
        for (entity, visual, mut visibility, mut node) in &mut status_nodes {
            if !entity_has_ancestor(entity, row_entity, &parents) {
                continue;
            }
            let expected_status = ScenarioObjectiveStatusFilter::from(&visual.0);
            visibility.set_if_neq(if expected_status.includes_objective_status(*status) {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
            // Each status list box is an icon column and a label authored
            // `x="30" walign="max"`: the label fills the rest of the row and
            // wraps at that width, so its measured height is its drawn height.
            let columns = vec![RepeatedGridTrack::auto(1), RepeatedGridTrack::fr(1, 1.0)];
            if node.grid_template_columns != columns || node.justify_items != JustifyItems::Stretch
            {
                node.grid_template_columns = columns;
                node.justify_items = JustifyItems::Stretch;
            }
        }
    }
}

/// Applies objective category and status filters without copying objective data.
pub(super) fn apply_scenario_goal_panel_filters_to_projected_rows(
    scenarios: Res<WorldScenarios>,
    scenario_assets: Res<Assets<WorldScenarioDocumentAsset>>,
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
    let scenarios = scenarios.get(&scenario_assets);
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
                || scenarios
                    .and_then(|scenarios| scenarios.scenario(objective.scenario))
                    .and_then(|scenario| scenario.objectives.get(objective.record_index as usize))
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
