use bevy::{
    ecs::{entity::EntityHashSet, relationship::RelationshipTarget},
    prelude::*,
};
use openzt2_game_assets::{
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    world_scenario::{
        world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios,
        world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
    },
};
use openzt2_game_data::{
    AssetId,
    ui_document::node_property_binding::{
        UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource, UiTextPropertyBindingSource,
    },
};
use openzt2_interface_state::ui::authored_ui_node_projection_components::{
    UiValue, UiValueBinding, UiVisibleBinding,
};
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::{
        world_load_completion_marker::WorldLoadCompleted,
        world_membership_types::{WorldMember, WorldMembers, WorldRoot},
    },
};
use openzt2_zoo_state::scenario::{
    scenario_objective_types::{
        ScenarioObjective, ScenarioObjectiveDeadline, ScenarioObjectiveProgress,
        ScenarioObjectiveStatus,
    },
    scenario_session_types::{ActiveScenarioSession, ScenarioDocument, ScenarioSessionState},
};

#[derive(Component, Default, PartialEq, Eq)]
pub(crate) struct ScenarioPresentationObjectives {
    by_id: std::collections::BTreeMap<AssetId, Entity>,
    earliest_deadline: Option<u64>,
}

pub(crate) fn bind_scenario_presentation_objectives(
    sources: Res<WorldScenarios>,
    documents: Res<Assets<WorldScenarioDocumentAsset>>,
    mut roots: Query<
        (
            Entity,
            Option<Ref<WorldMembers>>,
            &mut ScenarioPresentationObjectives,
        ),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
    objectives: Query<(&ScenarioObjective, Option<&ScenarioObjectiveDeadline>)>,
    changed: Query<
        &WorldMember,
        (
            With<ScenarioObjective>,
            Or<(
                Changed<ScenarioObjective>,
                Changed<ScenarioObjectiveDeadline>,
            )>,
        ),
    >,
    members: Query<&WorldMember>,
    mut removed: (
        RemovedComponents<ScenarioObjective>,
        RemovedComponents<ScenarioObjectiveDeadline>,
        RemovedComponents<WorldMembers>,
    ),
) {
    let authors_changed = sources.is_changed() || documents.is_changed();
    let affected: EntityHashSet = changed
        .iter()
        .map(|member| member.root)
        .chain(
            members
                .iter_many(removed.0.read().chain(removed.1.read()))
                .flatten()
                .map(|member| member.root),
        )
        .chain(removed.2.read())
        .collect();
    let view = sources.get(&documents);
    for (root, members, mut index) in &mut roots {
        if !authors_changed
            && !index.is_added()
            && !affected.contains(&root)
            && !members.as_ref().is_some_and(Ref::is_changed)
        {
            continue;
        }
        let mut next = ScenarioPresentationObjectives::default();
        for entity in members
            .as_deref()
            .into_iter()
            .flat_map(RelationshipTarget::iter)
        {
            let Ok((objective, deadline)) = objectives.get(entity) else {
                continue;
            };
            if let Some(deadline) = deadline {
                next.earliest_deadline = Some(
                    next.earliest_deadline
                        .map_or(deadline.end_tick, |previous| {
                            previous.min(deadline.end_tick)
                        }),
                );
            }
            let Some(record) = view
                .as_ref()
                .and_then(|view| view.scenario(objective.scenario))
                .and_then(|scenario| scenario.objectives.get(objective.record_index as usize))
            else {
                continue;
            };
            next.by_id.entry(record.id).or_insert(entity);
        }
        index.set_if_neq(next);
    }
}

pub(crate) fn project_active_scenario_status_to_authored_information_bindings(
    scenario_documents: Res<Assets<WorldScenarioDocumentAsset>>,
    world_scenarios: Res<WorldScenarios>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    world: Query<
        (
            Ref<ScenarioPresentationObjectives>,
            Ref<ZooClock>,
            Option<Ref<ActiveScenarioSession>>,
            Option<Ref<ScenarioDocument>>,
        ),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
    objectives: Query<(
        &ScenarioObjective,
        Ref<ScenarioObjectiveStatus>,
        Ref<ScenarioObjectiveProgress>,
    )>,
    authored: (
        Query<(Ref<UiTextBinding>, &mut Text)>,
        Query<(Ref<UiValueBinding>, &mut UiValue)>,
        Query<(Ref<UiVisibleBinding>, &mut Visibility)>,
    ),
) {
    let Ok((index, clock, active, selection)) = world.single() else {
        return;
    };
    let (mut text, mut values, mut visibility) = authored;
    let source_changed = world_scenarios.is_changed() || scenario_documents.is_changed();
    let refresh_all = index.is_changed()
        || source_changed
        || active.as_ref().is_some_and(Ref::is_changed)
        || selection.as_ref().is_some_and(Ref::is_changed);
    let objective = |id: AssetId| {
        index
            .by_id
            .get(&id)
            .and_then(|&entity| objectives.get(entity).ok())
    };
    let running = active
        .as_ref()
        .is_some_and(|session| session.state == ScenarioSessionState::Running);

    for (binding, mut state) in &mut visibility {
        let visible = match binding.0 {
            UiBooleanPropertyBindingSource::ScenarioRunning
                if refresh_all || binding.is_changed() =>
            {
                running
            }
            UiBooleanPropertyBindingSource::ScenarioObjectiveSatisfied { objective: id } => {
                let state = objective(id);
                if !refresh_all
                    && !binding.is_changed()
                    && !state
                        .as_ref()
                        .is_some_and(|(_, status, _)| status.is_changed())
                {
                    continue;
                }
                state.is_some_and(|(_, status, _)| *status == ScenarioObjectiveStatus::Satisfied)
            }
            _ => continue,
        };
        state.set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }

    let deadline = index.earliest_deadline.map_or(0, |tick| {
        i64::try_from(tick.saturating_sub(clock.tick)).unwrap_or_default()
    });
    for (binding, mut projected) in &mut values {
        let value = match binding.0 {
            UiIntegerPropertyBindingSource::ScenarioObjectiveCurrent { objective: id }
            | UiIntegerPropertyBindingSource::ScenarioObjectiveTarget { objective: id } => {
                let state = objective(id);
                if !refresh_all
                    && !binding.is_changed()
                    && !state
                        .as_ref()
                        .is_some_and(|(_, _, progress)| progress.is_changed())
                {
                    continue;
                }
                let target = matches!(
                    binding.0,
                    UiIntegerPropertyBindingSource::ScenarioObjectiveTarget { .. }
                );
                state.map_or(0, |(_, _, progress)| {
                    if target {
                        progress.target
                    } else {
                        progress.current
                    }
                })
            }
            UiIntegerPropertyBindingSource::ScenarioTimeRemainingTicks
                if refresh_all || binding.is_changed() || clock.is_changed() =>
            {
                deadline
            }
            _ => continue,
        };
        projected.set_if_neq(UiValue(value));
    }

    let Some(localization) =
        active_localization.borrow_loaded_localization_view(&localization_assets)
    else {
        return;
    };
    let locale_changed = active_localization.is_changed() || localization_assets.is_changed();
    let authored_scenarios = world_scenarios.get(&scenario_documents);
    for (binding, projected) in &mut text {
        let refresh = refresh_all || locale_changed || binding.is_changed();
        let key = match binding.0 {
            UiTextPropertyBindingSource::ScenarioObjectiveText { objective: id } => {
                let state = objective(id);
                if !refresh
                    && !state
                        .as_ref()
                        .is_some_and(|(_, status, _)| status.is_changed())
                {
                    continue;
                }
                state.and_then(|(objective, status, _)| {
                    let record = authored_scenarios
                        .as_ref()?
                        .scenario(objective.scenario)?
                        .objectives
                        .get(objective.record_index as usize)?;
                    Some(match *status {
                        ScenarioObjectiveStatus::Satisfied
                            if record.success_text_key != AssetId::default() =>
                        {
                            record.success_text_key
                        }
                        ScenarioObjectiveStatus::Failed
                            if record.failure_text_key != AssetId::default() =>
                        {
                            record.failure_text_key
                        }
                        _ => record.text_key,
                    })
                })
            }
            UiTextPropertyBindingSource::ScenarioDescription if refresh => active
                .as_ref()
                .zip(selection.as_ref())
                .and_then(|(session, selection)| {
                    scenario_documents
                        .get(&selection.0)?
                        .document
                        .find_scenario_record(session.definition)
                })
                .map(|scenario| scenario.description_key),
            _ => continue,
        };
        let mut formatted = String::new();
        if let Some(key) = key {
            let _ =
                localization.write_localized_text_with_format_arguments(key, &[], &mut formatted);
        }
        super::text_replacement_operations::replace_projected_ui_text_if_changed(
            projected, &formatted,
        );
    }
}
