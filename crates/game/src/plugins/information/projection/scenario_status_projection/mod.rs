use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::node_property_binding::{
        UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource, UiTextPropertyBindingSource,
    },
    AssetId,
};

use crate::assets::localization::localization_asset_types::LocalizationAsset;
use crate::assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::plugins::scenario::scenario_objective_types::ScenarioObjective;
use crate::plugins::scenario::scenario_objective_types::ScenarioObjectiveDeadline;
use crate::plugins::scenario::scenario_objective_types::ScenarioObjectiveProgress;
use crate::plugins::scenario::scenario_objective_types::ScenarioObjectiveStatus;
use crate::plugins::scenario::scenario_session_types::ActiveScenarioSession;
use crate::plugins::scenario::scenario_session_types::ScenarioSessionState;
use crate::plugins::scenario::scenario_session_types::SelectedScenarioDocument;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::ui::authored_ui_node_projection_components::UiValue;
use crate::plugins::ui::authored_ui_node_projection_components::UiValueBinding;
use crate::plugins::ui::authored_ui_node_projection_components::UiVisibleBinding;
use crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding;

// Scenario bindings need both objective state and localized presentation data.
#[allow(clippy::too_many_arguments)]
pub(in crate::plugins::information) fn project_active_scenario_status_to_authored_information_bindings(
    selected_scenario_document: Option<Res<SelectedScenarioDocument>>,
    scenario_documents: Res<Assets<WorldScenarioDocumentAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    zoo_clock: Res<ZooClock>,
    active_scenario_sessions: Query<&ActiveScenarioSession>,
    scenario_objectives: Query<(
        &ScenarioObjective,
        &ScenarioObjectiveStatus,
        &ScenarioObjectiveProgress,
        Option<&ScenarioObjectiveDeadline>,
    )>,
    mut authored_text: Query<(&UiTextBinding, &mut Text)>,
    mut authored_integer_values: Query<(&UiValueBinding, &mut UiValue)>,
    mut authored_boolean_visibility: Query<(&UiVisibleBinding, &mut Visibility)>,
) {
    let Some(scenario_document_asset) = selected_scenario_document
        .as_deref()
        .and_then(|selection| scenario_documents.get(&selection.0))
    else {
        return;
    };
    let scenario_document = &scenario_document_asset.document;
    let active_scenario_session = active_scenario_sessions.iter().next();
    let scenario_is_running = active_scenario_sessions
        .iter()
        .any(|session| matches!(session.state, ScenarioSessionState::Running));

    for (property_binding, mut projected_visibility) in &mut authored_boolean_visibility {
        let should_be_visible = match &property_binding.0 {
            UiBooleanPropertyBindingSource::ScenarioRunning => scenario_is_running,
            UiBooleanPropertyBindingSource::ScenarioObjectiveSatisfied { objective } => {
                find_scenario_objective_status_by_authored_identifier(
                    scenario_document,
                    *objective,
                    &scenario_objectives,
                )
                .is_some_and(|(_, status, _, _)| *status == ScenarioObjectiveStatus::Satisfied)
            }
            _ => continue,
        };
        *projected_visibility = if should_be_visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (property_binding, mut projected_value) in &mut authored_integer_values {
        projected_value.0 = match &property_binding.0 {
            UiIntegerPropertyBindingSource::ScenarioObjectiveCurrent { objective } => {
                find_scenario_objective_status_by_authored_identifier(
                    scenario_document,
                    *objective,
                    &scenario_objectives,
                )
                .map_or(0, |(_, _, progress, _)| progress.current)
            }
            UiIntegerPropertyBindingSource::ScenarioObjectiveTarget { objective } => {
                find_scenario_objective_status_by_authored_identifier(
                    scenario_document,
                    *objective,
                    &scenario_objectives,
                )
                .map_or(0, |(_, _, progress, _)| progress.target)
            }
            UiIntegerPropertyBindingSource::ScenarioTimeRemainingTicks => scenario_objectives
                .iter()
                .filter_map(|(_, _, _, deadline)| deadline)
                .map(|deadline| deadline.end_tick.saturating_sub(zoo_clock.tick))
                .min()
                .and_then(|remaining_ticks| i64::try_from(remaining_ticks).ok())
                .unwrap_or(0),
            _ => continue,
        };
    }

    let Some(localization_catalogue) =
        active_localization.borrow_loaded_localization_view(&localization_assets)
    else {
        return;
    };
    for (property_binding, mut projected_text) in &mut authored_text {
        let localization_key = match &property_binding.0 {
            UiTextPropertyBindingSource::ScenarioDescription => active_scenario_session
                .and_then(|session| scenario_document.find_scenario_record(session.definition))
                .map(|scenario| AssetId(scenario.description_key.0)),
            UiTextPropertyBindingSource::ScenarioObjectiveText { objective } => {
                find_scenario_objective_status_by_authored_identifier(
                    scenario_document,
                    *objective,
                    &scenario_objectives,
                )
                .map(|(record, status, _, _)| match *status {
                    ScenarioObjectiveStatus::Satisfied if record.success_text_key.0 != [0; 16] => {
                        AssetId(record.success_text_key.0)
                    }
                    ScenarioObjectiveStatus::Failed if record.failure_text_key.0 != [0; 16] => {
                        AssetId(record.failure_text_key.0)
                    }
                    _ => AssetId(record.text_key.0),
                })
            }
            _ => continue,
        };
        let Some(localization_key) = localization_key else {
            continue;
        };
        projected_text.0.clear();
        let _ = localization_catalogue.write_localized_text_with_format_arguments(
            localization_key,
            &[],
            &mut projected_text.0,
        );
    }
}

fn find_scenario_objective_status_by_authored_identifier<'a>(
    scenario_document: &'a openzt2_game_data::world_scenario::WorldScenarioDocument,
    objective_identifier: AssetId,
    scenario_objectives: &'a Query<(
        &ScenarioObjective,
        &ScenarioObjectiveStatus,
        &ScenarioObjectiveProgress,
        Option<&ScenarioObjectiveDeadline>,
    )>,
) -> Option<(
    &'a openzt2_game_data::world_scenario::ScenarioObjectiveRecord,
    &'a ScenarioObjectiveStatus,
    &'a ScenarioObjectiveProgress,
    Option<&'a ScenarioObjectiveDeadline>,
)> {
    scenario_objectives
        .iter()
        .find_map(|(objective, status, progress, deadline)| {
            let authored_objective = scenario_document
                .find_scenario_objective(objective.scenario, objective.record_index)?;
            (authored_objective.id.0 == objective_identifier.0).then_some((
                authored_objective,
                status,
                progress,
                deadline,
            ))
        })
}
