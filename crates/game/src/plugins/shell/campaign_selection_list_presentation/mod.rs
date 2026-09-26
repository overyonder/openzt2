use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::widget_live_collection::UiWidgetLiveCollectionSource, AssetId,
};

use crate::assets::localization::localization_asset_types::LocalizationAsset;
use crate::assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use crate::assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::game_session_types::WorldSessionMode;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::SetUiListRowCount;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use crate::plugins::ui::authored_ui_change_activation_dispatch::UiPreviousSelection;
use crate::plugins::ui::authored_ui_selection_state::UiSelected;

use super::{
    shell_selection_types::{ShellSelection, WorldChoice, WorldChoiceView},
    world_selection_presentation_types::CampaignChoice,
};

pub(super) fn request_authored_campaign_and_scenario_list_row_counts(
    catalogue_source: Res<WorldScenarios>,
    catalogues: Res<Assets<WorldScenarioDocumentAsset>>,
    lists: Query<(Entity, &UiListPolicy)>,
    selected: Query<(&CampaignChoice, &UiSelected)>,
    mut requests: MessageWriter<SetUiListRowCount>,
) {
    let Some(catalogue) = catalogue_source.get(&catalogues) else {
        return;
    };
    let selected_campaign = selected
        .iter()
        .find_map(|(campaign, selected)| selected.0.then_some(campaign.0));
    for (entity, policy) in &lists {
        match policy.source {
            UiWidgetLiveCollectionSource::Campaigns => {
                requests.write(SetUiListRowCount {
                    list: entity,
                    count: catalogue.campaigns().count().min(u16::MAX as usize) as u16,
                });
            }
            UiWidgetLiveCollectionSource::CampaignScenarios => {
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
        &UiListRow,
        Option<&CampaignChoice>,
        Option<&WorldChoiceView>,
        &mut Visibility,
    )>,
    mut selection: ResMut<ShellSelection>,
    children: Query<&Children>,
    mut texts: Query<&mut Text>,
    choices: Query<(Entity, &WorldChoice)>,
    selected_campaign: Query<(&CampaignChoice, &UiSelected)>,
    active: Option<Res<LocalizationPrecedenceIndex>>,
    localizations: Res<Assets<LocalizationAsset>>,
) {
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
    if let Some(campaign) = active_campaign.and_then(|id| catalogue.campaign(id)) {
        let launchable_scenarios = || {
            campaign.scenarios.iter().filter(|scenario| {
                choices
                    .iter()
                    .any(|(_, choice)| choice.scenario == scenario.id)
            })
        };
        if !launchable_scenarios().any(|scenario| Some(scenario.id) == selection.scenario) {
            let next = launchable_scenarios().next().map(|scenario| scenario.id);
            if selection.scenario != next {
                selection.scenario = next;
            }
        }
    }
    for (entity, row, current_campaign, current_scenario, mut visibility) in &mut rows {
        let Ok(policy) = lists.get(row.list) else {
            continue;
        };
        match policy.source {
            UiWidgetLiveCollectionSource::Campaigns => {
                let Some(campaign) = catalogue.campaigns().nth(usize::from(row.index)) else {
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
                let Some(source) = choices.iter().find_map(|(entity, choice)| {
                    (choice.scenario == scenario.id).then_some(entity)
                }) else {
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

pub(super) fn replace_text_in_list_row_and_descendants(
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
