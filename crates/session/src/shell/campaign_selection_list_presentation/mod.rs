use std::collections::HashMap;

use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::widget_live_collection::UiWidgetLiveCollectionSource, AssetId,
};

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::SetUiListRowCount;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;
use openzt2_ui::authored_ui_change_activation_dispatch::UiPreviousSelection;
use openzt2_world_state::game_session_types::WorldSessionMode;

use openzt2_interface_state::shell::shell_selection_types::ShellSelection;
use openzt2_interface_state::shell::shell_selection_types::WorldChoice;
use openzt2_interface_state::shell::shell_selection_types::WorldChoiceView;

use super::world_selection_presentation_types::CampaignChoice;

pub(super) fn request_authored_campaign_and_scenario_list_row_counts(
    catalogue_source: Res<WorldScenarios>,
    catalogues: Res<Assets<WorldScenarioDocumentAsset>>,
    lists: Query<(Entity, Ref<UiListPolicy>)>,
    selected: Query<(&CampaignChoice, Ref<UiSelected>)>,
    mut requests: MessageWriter<SetUiListRowCount>,
) {
    let Some(catalogue) = catalogue_source.get(&catalogues) else {
        return;
    };
    let catalogue_changed = catalogue_source.is_changed() || catalogues.is_changed();
    let campaign_changed = selected.iter().any(|(_, selected)| selected.is_changed());
    let selected_campaign = selected
        .iter()
        .find_map(|(campaign, selected)| selected.0.then_some(campaign.0));
    for (entity, policy) in &lists {
        match policy.source {
            UiWidgetLiveCollectionSource::Campaigns if catalogue_changed || policy.is_added() => {
                requests.write(SetUiListRowCount {
                    list: entity,
                    count: catalogue.campaigns().count().min(u16::MAX as usize) as u16,
                });
            }
            UiWidgetLiveCollectionSource::CampaignScenarios
                if catalogue_changed || campaign_changed || policy.is_added() =>
            {
                let Some(campaign) = selected_campaign else {
                    continue;
                };
                let Some(record) = catalogue.campaign(campaign) else {
                    continue;
                };
                requests.write(SetUiListRowCount {
                    list: entity,
                    count: record.scenarios.len().min(u16::MAX as usize) as u16,
                });
            }
            _ => {}
        }
    }
}

pub(super) fn project_campaign_and_scenario_records_into_authored_list_rows(
    mut commands: Commands,
    catalogue_source: Res<WorldScenarios>,
    catalogues: Res<Assets<WorldScenarioDocumentAsset>>,
    lists: Query<&UiListPolicy>,
    mut rows: Query<(
        Entity,
        Ref<UiListRow>,
        &ChildOf,
        Option<&CampaignChoice>,
        Option<&WorldChoiceView>,
        &mut Visibility,
    )>,
    mut selection: ResMut<ShellSelection>,
    children: Query<&Children>,
    mut texts: Query<&mut Text>,
    choices: Query<(Entity, Ref<WorldChoice>)>,
    selected_campaign: Query<(&CampaignChoice, Ref<UiSelected>)>,
    mut removed_choices: RemovedComponents<WorldChoice>,
    active: Option<Res<LocalizationPrecedenceIndex>>,
    localizations: Res<Assets<LocalizationAsset>>,
    profile_variables: Res<
        openzt2_zoo_state::progression::profile_scenario_variables::ProfileScenarioVariables,
    >,
) {
    let choices_removed = removed_choices.read().count() != 0;
    if selection.mode != Some(WorldSessionMode::Campaign) {
        return;
    }
    let Some(catalogue) = catalogue_source.get(&catalogues) else {
        return;
    };
    let Some(localization) = active
        .as_deref()
        .and_then(|sources| sources.borrow_loaded_localization_view(&localizations))
    else {
        return;
    };
    let selected_campaign_id = selected_campaign
        .iter()
        .find_map(|(campaign, selected)| selected.0.then_some(campaign.0));
    let active_campaign = selected_campaign_id
        .filter(|id| catalogue.campaign(*id).is_some())
        .or_else(|| catalogue.campaigns().next().map(|campaign| campaign.id));
    let refresh_rows = catalogue_source.is_changed()
        || catalogues.is_changed()
        || active.as_ref().is_some_and(|active| active.is_changed())
        || localizations.is_changed()
        || profile_variables.is_changed()
        || selection.is_changed()
        || choices_removed
        || selected_campaign
            .iter()
            .any(|(_, selected)| selected.is_changed())
        || choices.iter().any(|(_, choice)| choice.is_changed());
    if !refresh_rows && !rows.iter().any(|(_, row, ..)| row.is_added()) {
        return;
    }
    let campaigns = catalogue.campaigns().collect::<Vec<_>>();
    let choice_entities = choices
        .iter()
        .fold(HashMap::new(), |mut entities, (entity, choice)| {
            if choice.mode == WorldSessionMode::Campaign {
                entities.entry(choice.scenario).or_insert(entity);
            }
            entities
        });
    if let Some(campaign) = active_campaign.and_then(|id| catalogue.campaign(id)) {
        let launchable_scenarios = || {
            campaign.scenarios.iter().filter(|scenario| {
                profile_variables.scenario_state(scenario)
                    != openzt2_game_data::world_scenario::CampaignScenarioState::Locked
                    && choice_entities.contains_key(&scenario.id)
            })
        };
        if !launchable_scenarios().any(|scenario| Some(scenario.id) == selection.scenario) {
            let next = launchable_scenarios().next().map(|scenario| scenario.id);
            if selection.scenario != next {
                selection.scenario = next;
            }
        }
    }
    for (entity, row, parent, current_campaign, current_scenario, mut visibility) in &mut rows {
        if !refresh_rows && !row.is_added() {
            continue;
        }
        let Ok(policy) = lists.get(parent.parent()) else {
            continue;
        };
        match policy.source {
            UiWidgetLiveCollectionSource::Campaigns => {
                let Some(campaign) = campaigns.get(usize::from(row.index)) else {
                    visibility.set_if_neq(Visibility::Hidden);
                    commands
                        .entity(entity)
                        .remove::<(CampaignChoice, UiSelected, UiPreviousSelection)>();
                    continue;
                };
                visibility.set_if_neq(Visibility::Inherited);
                if current_campaign.is_none_or(|current| current.0 != campaign.id) {
                    commands.entity(entity).insert((
                        CampaignChoice(campaign.id),
                        UiSelected(Some(campaign.id) == active_campaign),
                        UiPreviousSelection::from_current_selection(
                            Some(campaign.id) == active_campaign,
                        ),
                    ));
                } else if selected_campaign_id != active_campaign {
                    commands.entity(entity).insert((
                        UiSelected(Some(campaign.id) == active_campaign),
                        UiPreviousSelection::from_current_selection(
                            Some(campaign.id) == active_campaign,
                        ),
                    ));
                }
                replace_text_in_list_row_and_descendants(
                    entity,
                    localization
                        .find_plain_localized_text(AssetId(campaign.name_key.0))
                        .unwrap_or("Unnamed campaign"),
                    &children,
                    &mut texts,
                );
            }
            UiWidgetLiveCollectionSource::CampaignScenarios => {
                let Some(campaign) = active_campaign.and_then(|id| catalogue.campaign(id)) else {
                    visibility.set_if_neq(Visibility::Hidden);
                    commands.entity(entity).remove::<WorldChoiceView>();
                    continue;
                };
                let Some(scenario) = campaign.scenarios.get(usize::from(row.index)) else {
                    visibility.set_if_neq(Visibility::Hidden);
                    commands.entity(entity).remove::<WorldChoiceView>();
                    continue;
                };
                commands.entity(entity).insert(openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled(
                    profile_variables.scenario_state(scenario) != openzt2_game_data::world_scenario::CampaignScenarioState::Locked,
                ));
                let Some(&source) = choice_entities.get(&scenario.id) else {
                    visibility.set_if_neq(Visibility::Hidden);
                    commands.entity(entity).remove::<WorldChoiceView>();
                    continue;
                };
                visibility.set_if_neq(Visibility::Inherited);
                if current_scenario.is_none_or(|current| current.0 != source) {
                    commands.entity(entity).insert((
                        WorldChoiceView(source),
                        UiSelected(selection.scenario == Some(scenario.id)),
                        UiPreviousSelection::from_current_selection(
                            selection.scenario == Some(scenario.id),
                        ),
                    ));
                }
                replace_text_in_list_row_and_descendants(
                    entity,
                    localization
                        .find_plain_localized_text(AssetId(scenario.name_key.0))
                        .unwrap_or("Unnamed scenario"),
                    &children,
                    &mut texts,
                );
            }
            _ => {}
        }
    }
}

pub(crate) fn replace_text_in_list_row_and_descendants(
    entity: Entity,
    value: &str,
    children: &Query<&Children>,
    texts: &mut Query<&mut Text>,
) {
    if let Ok(mut text) = texts.get_mut(entity) {
        if text.0 != value {
            text.0.clear();
            text.0.push_str(value);
        }
    }
    if let Ok(entity_children) = children.get(entity) {
        entity_children.iter().for_each(|child| {
            replace_text_in_list_row_and_descendants(child, value, children, texts);
        });
    }
}
