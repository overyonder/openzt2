use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::node_property_binding::{
        UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource, UiTextPropertyBindingSource,
    },
    AssetId,
};

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValueBinding;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiVisibleBinding;
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjective;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveDeadline;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveProgress;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatus;
use openzt2_zoo_state::scenario::scenario_session_types::ActiveScenarioSession;
use openzt2_zoo_state::scenario::scenario_session_types::ScenarioSessionState;
use openzt2_zoo_state::scenario::scenario_session_types::SelectedScenarioDocument;

// Scenario bindings need both objective state and localized presentation data.
#[allow(clippy::too_many_arguments)]
pub(crate) fn project_active_scenario_status_to_authored_information_bindings(
    selected_scenario_document: Option<Res<SelectedScenarioDocument>>,
    scenario_documents: Res<Assets<WorldScenarioDocumentAsset>>,
    world_scenarios: Res<WorldScenarios>,
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
    mut formatted_text: Local<String>,
) {
    let world_scenarios = world_scenarios.get(&scenario_documents);
    // Challenge maps have no selected campaign document, but their accepted
    // challenges' objective text still projects into the goal list.
    let selected_scenario_document = selected_scenario_document
        .as_deref()
        .and_then(|selection| scenario_documents.get(&selection.0))
        .map(|asset| &asset.document);
    let localization_catalogue =
        active_localization.borrow_loaded_localization_view(&localization_assets);
    let Some(localization_catalogue) = localization_catalogue else {
        return;
    };
    for (property_binding, mut projected_text) in &mut authored_text {
        let UiTextPropertyBindingSource::ScenarioObjectiveText { objective } = &property_binding.0
        else {
            continue;
        };
        let Some(localization_key) =
            scenario_objectives
                .iter()
                .find_map(|(live_objective, status, _, _)| {
                    let record = world_scenarios?
                        .scenario(live_objective.scenario)?
                        .objectives
                        .get(live_objective.record_index as usize)?;
                    (record.id.0 == objective.0).then(|| match *status {
                        ScenarioObjectiveStatus::Satisfied
                            if record.success_text_key.0 != [0; 16] =>
                        {
                            AssetId(record.success_text_key.0)
                        }
                        ScenarioObjectiveStatus::Failed if record.failure_text_key.0 != [0; 16] => {
                            AssetId(record.failure_text_key.0)
                        }
                        _ => AssetId(record.text_key.0),
                    })
                })
        else {
            continue;
        };
        formatted_text.clear();
        let _ = localization_catalogue.write_localized_text_with_format_arguments(
            localization_key,
            &[],
            &mut *formatted_text,
        );
        if projected_text.0 != *formatted_text {
            super::text_replacement_operations::replace_projected_ui_text_if_changed(
                &mut projected_text.0,
                &formatted_text,
            );
        }
    }
    let Some(scenario_document) = selected_scenario_document else {
        return;
    };
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

    for (property_binding, mut projected_text) in &mut authored_text {
        let localization_key = match &property_binding.0 {
            UiTextPropertyBindingSource::ScenarioDescription => active_scenario_session
                .and_then(|session| scenario_document.find_scenario_record(session.definition))
                .map(|scenario| AssetId(scenario.description_key.0)),
            _ => continue,
        };
        let Some(localization_key) = localization_key else {
            continue;
        };
        formatted_text.clear();
        let _ = localization_catalogue.write_localized_text_with_format_arguments(
            localization_key,
            &[],
            &mut *formatted_text,
        );
        if projected_text.0 != *formatted_text {
            super::text_replacement_operations::replace_projected_ui_text_if_changed(
                &mut projected_text.0,
                &formatted_text,
            );
        }
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
