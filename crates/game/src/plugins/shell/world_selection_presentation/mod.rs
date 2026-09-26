use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    action::shell_navigation::UiShellAction, node_property_binding::*,
};
use openzt2_game_data::AssetId;

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::game_session_types::WorldSessionMode;
use crate::plugins::ui::authored_globe_presentation_types::UiGlobeBiomeModel;
use crate::plugins::ui::authored_globe_presentation_types::UiGlobeMarkerVisual;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use crate::plugins::ui::authored_ui_action_projection_components::UiShellActions;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;
use crate::plugins::ui::authored_ui_selection_state::UiSelected;

use super::{
    shell_selection_types::{ShellSelection, WorldChoice, WorldChoiceView},
    world_selection_presentation_types::{
        SecondaryGlobe, SelectedWorldCatalogueFilter, WorldSelectionParams,
    },
};

/// Selects the toggle matching the current catalogue filter.
pub(super) fn project_selected_world_catalogue_filter_into_authored_location_toggle(
    filters: Query<&SelectedWorldCatalogueFilter>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<(&UiDocumentRoot, &ChildOf)>,
    mut action_nodes: Query<
        (&UiShellActions, &UiDocumentOwner, &mut UiSelected),
        Without<WorldChoiceView>,
    >,
) {
    for (actions, document_owner, mut selected) in &mut action_nodes {
        let Some((document, filter)) =
            roots.get(document_owner.0).ok().and_then(|(root, owner)| {
                documents
                    .get(&root.document)
                    .zip(filters.get(owner.parent()).ok())
            })
        else {
            continue;
        };
        let matches_selected_filter = actions
            .authored_action_records(document)
            .filter_map(|record| match &record.action {
                UiShellAction::FilterWorldChoicesToLocation { world_location } => {
                    Some(if *world_location == AssetId::default() {
                        matches!(filter, SelectedWorldCatalogueFilter::All)
                    } else {
                        matches!(
                            filter,
                            SelectedWorldCatalogueFilter::BiomeLocationGroup(selected)
                                if selected == world_location
                        )
                    })
                }
                UiShellAction::FilterWorldChoicesToExpansionPack {
                    expansion_pack_identifier,
                } => Some(matches!(
                    filter,
                    SelectedWorldCatalogueFilter::ExpansionPack(selected)
                        if selected == expansion_pack_identifier
                )),
                _ => None,
            })
            .next();
        if let Some(matches_selected_filter) = matches_selected_filter {
            if selected.0 != matches_selected_filter {
                selected.0 = matches_selected_filter;
            }
        }
    }
}

pub(super) fn filter_world_choice_rows_by_selected_catalogue_filter(
    filters: Query<Ref<SelectedWorldCatalogueFilter>>,
    active_catalogue: Res<WorldScenarios>,
    catalogues: Res<Assets<WorldScenarioDocumentAsset>>,
    choices: Query<&WorldChoice>,
    mut rows: Query<
        (Ref<WorldChoiceView>, &mut Visibility, &mut Node),
        (With<UiListRow>, Without<UiGlobeMarkerVisual>),
    >,
    mut selection: ResMut<ShellSelection>,
) {
    if selection.mode == Some(WorldSessionMode::Campaign) {
        return;
    }
    let Some(catalogue) = active_catalogue.get(&catalogues) else {
        return;
    };
    let Some(filter) = filters.iter().next() else {
        return;
    };
    let filter_changed = filter.is_changed();
    if !filter_changed
        && !catalogues.is_changed()
        && rows.iter().all(|(view, _, _)| !view.is_changed())
    {
        return;
    }
    let includes = |choice: &WorldChoice| {
        catalogue.map(choice.map).is_some_and(|map| match *filter {
            SelectedWorldCatalogueFilter::All => true,
            SelectedWorldCatalogueFilter::BiomeLocationGroup(biome) => {
                AssetId(map.biome.0) == biome
            }
            SelectedWorldCatalogueFilter::ExpansionPack(expansion_pack_identifier) => {
                map.expansion_pack_filter_identifier == expansion_pack_identifier
            }
        })
    };
    let selected_is_visible = selection.scenario.is_some_and(|scenario| {
        choices
            .iter()
            .any(|choice| choice.scenario == scenario && includes(choice))
    });
    // Changing the filter selects its first row, even if the old selection
    // remains visible. This also turns the globe back after a manual drag.
    if filter_changed || !selected_is_visible {
        selection.scenario = choices
            .iter()
            .filter(|choice| includes(choice))
            .filter_map(|choice| {
                catalogue
                    .map(choice.map)
                    .map(|map| (map.catalogue_order, choice.scenario.0, choice.scenario))
            })
            .min_by_key(|(order, scenario, _)| (*order, *scenario))
            .map(|(_, _, scenario)| scenario);
    }
    for (view, mut visibility, mut node) in &mut rows {
        let visible = choices.get(view.0).is_ok_and(&includes);
        *visibility = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        node.display = if visible {
            Display::Flex
        } else {
            Display::None
        };
    }
}

pub(super) fn show_only_selected_secondary_globe_biome_model(
    selected: Query<Ref<SecondaryGlobe>>,
    mut models: Query<(Ref<UiGlobeBiomeModel>, &mut Visibility)>,
) {
    let explicit = selected.iter().next();
    let selected_biome = explicit
        .as_deref()
        .map(|selected| selected.0)
        .filter(|id| *id != AssetId::default());
    if !explicit.is_some_and(|selected| selected.is_changed())
        && models.iter().all(|(model, _)| !model.is_added())
    {
        return;
    }
    for (model, mut visibility) in &mut models {
        *visibility = if selected_biome.is_some_and(|selected| model.biome == selected) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Updates choice highlights and details after the selected world changes.
pub(super) fn project_selected_world_facts_into_authored_map_selection_controls(
    params: WorldSelectionParams,
) {
    let WorldSelectionParams {
        mut commands,
        selection,
        profiles,
        active_catalogue,
        catalogues,
        documents,
        choices,
        mut selectable,
        text_bindings,
        image_bindings,
        mut visible_bindings,
        action_ranges,
        roots,
        changed_views,
        added_actions,
        added_text_bindings,
        added_image_bindings,
        added_visible_bindings,
        mut presentation,
        active,
        localizations,
    } = params;
    if !selection.is_changed()
        && !profiles.is_changed()
        && !active_catalogue.is_changed()
        && !catalogues.is_changed()
        && !localizations.is_changed()
        && !active.as_ref().is_some_and(|active| active.is_changed())
        && changed_views.is_empty()
        && added_actions.is_empty()
        && added_text_bindings.is_empty()
        && added_image_bindings.is_empty()
        && added_visible_bindings.is_empty()
    {
        return;
    }
    for (view, mut selected) in &mut selectable {
        let Ok(choice) = choices.get(view.0) else {
            continue;
        };
        let value =
            selection.mode == Some(choice.mode) && selection.scenario == Some(choice.scenario);
        if selected.0 != value {
            selected.0 = value;
        }
    }

    let catalogue = active_catalogue.get(&catalogues);
    let selected = catalogue.and_then(|catalogue| {
        selection.scenario.and_then(|scenario| {
            choices
                .iter()
                .find(|choice| choice.scenario == scenario && selection.mode == Some(choice.mode))
                .and_then(|choice| {
                    catalogue.map(choice.map).map(|map| {
                        (
                            catalogue,
                            map,
                            (choice.mode == WorldSessionMode::Campaign)
                                .then(|| catalogue.campaign_scenario(choice.scenario))
                                .flatten(),
                        )
                    })
                })
        })
    });
    let localization = active
        .as_deref()
        .and_then(|sources| sources.borrow_loaded_localization_view(&localizations));
    let localized = |key: Option<AssetId>| {
        key.and_then(|key| localization.and_then(|asset| asset.find_plain_localized_text(key)))
            .unwrap_or("")
    };
    let starting_cash = selected.and_then(|(catalogue, map, scenario)| {
        scenario
            .map(|scenario| scenario.starting_cash_cents)
            .or_else(|| {
                catalogue
                    .start(AssetId(map.starting_zoo.0))
                    .map(|start| start.cash_cents)
            })
            .map(|cash| {
                format_whole_currency_from_cents(selection.starting_cash_cents.unwrap_or(cash))
            })
    });
    for (binding, mut visibility) in &mut visible_bindings {
        let visible = match binding.0 {
            UiBooleanPropertyBindingSource::SelectedWorldAdjustableCash => {
                selection.mode == Some(WorldSessionMode::Challenge)
            }
            UiBooleanPropertyBindingSource::SelectedWorldUnlimitedCash => {
                selection.mode == Some(WorldSessionMode::Freeform)
            }
            UiBooleanPropertyBindingSource::SelectedWorldDifficulty => {
                selection.mode == Some(WorldSessionMode::Campaign)
            }
            _ => continue,
        };
        *visibility = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (entity, binding, children) in &text_bindings {
        let value = match binding.0 {
            UiTextPropertyBindingSource::PlayModeHeading => localized(selection.mode.map(|mode| {
                AssetId::from_key(match mode {
                    WorldSessionMode::Campaign => "mapdata:campaignheading",
                    WorldSessionMode::Challenge => "mapdata:challengeheading",
                    WorldSessionMode::Freeform => "mapdata:freeformheading",
                })
            })),
            UiTextPropertyBindingSource::SelectedWorldName => {
                selected.map_or("", |(_, map, scenario)| {
                    localized(Some(
                        scenario.map_or(map.name_key, |scenario| scenario.name_key),
                    ))
                })
            }
            UiTextPropertyBindingSource::SelectedWorldLocation => {
                selected.map_or("", |(catalogue, map, _)| {
                    localized(
                        catalogue
                            .location(map.location)
                            .map(|location| location.name_key),
                    )
                })
            }
            UiTextPropertyBindingSource::SelectedWorldBiome => {
                selected.map_or("", |(_, map, _)| localized(Some(AssetId(map.biome_key.0))))
            }
            UiTextPropertyBindingSource::SelectedWorldSize => {
                selected.map_or("", |(_, map, _)| localized(Some(AssetId(map.size_key.0))))
            }
            UiTextPropertyBindingSource::SelectedWorldDescription => {
                selected.map_or("", |(_, map, scenario)| {
                    let description_key = scenario
                        .map(|scenario| scenario.description_key)
                        .or_else(|| map.description_key.as_ref().map(|key| AssetId(key.0)));
                    if let Ok(mut node) = presentation.text_nodes.get_mut(entity) {
                        node.height = px(120);
                    }
                    localized(description_key)
                })
            }
            UiTextPropertyBindingSource::SelectedWorldStartingCash => {
                starting_cash.as_deref().unwrap_or("")
            }
            _ => continue,
        };
        let mut has_existing_bevy_text_presentation = false;
        if let Ok(mut text) = presentation.texts.get_mut(entity) {
            has_existing_bevy_text_presentation = true;
            if text.0.as_str() != value {
                **text = value.to_owned();
            }
        }
        if let Some(children) = children {
            for child in children.iter() {
                if let Ok(mut text) = presentation.texts.get_mut(child) {
                    has_existing_bevy_text_presentation = true;
                    if text.0.as_str() != value {
                        **text = value.to_owned();
                    }
                }
            }
        }
        if !has_existing_bevy_text_presentation {
            commands.entity(entity).insert(Text::new(value));
        }
    }
    for (entity, binding) in &image_bindings {
        if !matches!(
            binding.0,
            UiImagePropertyBindingSource::SelectedWorldThumbnail
        ) {
            continue;
        }
        if let Some((catalogue, map, _)) = selected {
            if let Some(handle) = catalogue.texture_image(AssetId(map.thumbnail.0)) {
                if let Ok(mut image) = presentation.images.get_mut(entity) {
                    image.image = handle;
                } else {
                    commands.entity(entity).insert(ImageNode::new(handle));
                }
            }
        }
    }

    for (entity, range, owner) in &action_ranges {
        let launches_selected_world = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
            .map(|document| {
                range.authored_action_records(document).any(|record| {
                    matches!(
                        record.action,
                        UiShellAction::SelectFreeformModeOrStartSelectedWorld
                            | UiShellAction::SelectChallengeModeOrStartSelectedWorld
                            | UiShellAction::SelectCampaignModeOrStartSelectedWorld
                            | UiShellAction::StartSelectedWorld
                    )
                })
            })
            .unwrap_or(false);
        if launches_selected_world {
            let enabled = profiles.selected_profile_identifier.is_some()
                && selection.mode.is_some()
                && selected.is_some();
            if let Ok(mut interaction_enabled) = presentation.enabled.get_mut(entity) {
                interaction_enabled.0 = enabled;
            }
            if let Ok(mut focusable) = presentation.focusable.get_mut(entity) {
                if focusable.enabled != enabled {
                    focusable.enabled = enabled;
                }
            }
            if !enabled {
                if let Ok(mut interaction) = presentation.interactions.get_mut(entity) {
                    *interaction = Interaction::None;
                }
            }
        }
    }
}

fn format_whole_currency_from_cents(cents: i64) -> String {
    let whole = cents / 100;
    let digits = whole.unsigned_abs().to_string();
    let grouped = digits
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(|chunk| std::str::from_utf8(chunk).expect("decimal digits are UTF-8"))
        .collect::<Vec<_>>()
        .join(",");
    format!("{}{grouped}", if whole < 0 { "-$" } else { "$" })
}
