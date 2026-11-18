use bevy::prelude::*;
use openzt2_game_data::ui_document::widget_live_collection::UiWidgetLiveCollectionSource;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerInteraction;

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use openzt2_interface_state::ui::authored_ui_focus_state::UiFocusPresentation;
use openzt2_interface_state::ui::authored_ui_focus_state::UiFocusable;
use openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;
use openzt2_ui::authored_toggle_selection_transitions::UiAuthoredToggleGroupSelectionPolicy;
use openzt2_ui::authored_ui_change_activation_dispatch::UiPreviousSelection;
use openzt2_ui::authored_ui_layout_participation::UiAuthoredLayoutDisplay;
use openzt2_ui::authored_ui_pointer_activation::UiPreviousPointerInteraction;
use openzt2_ui::authored_ui_pointer_activation::UiPreviousPointerPressTimeSeconds;
use openzt2_ui::projection::authored_document_projection::authored_first_toggle_list_row_index;
use openzt2_ui::projection::authored_document_projection::insert_authored_text_prototype;

use openzt2_interface_state::shell::shell_selection_types::GlobeMarker;
use openzt2_interface_state::shell::shell_selection_types::ShellSelection;
use openzt2_interface_state::shell::shell_selection_types::WorldChoice;
use openzt2_interface_state::shell::shell_selection_types::WorldChoiceView;

use super::campaign_selection_list_presentation::replace_text_in_list_row_and_descendants;

/// Populates the map toggle-set, which is empty in the UI document.
pub(super) fn project_launchable_world_choices_into_authored_list_rows(
    mut commands: Commands,
    active_catalogue: Res<WorldScenarios>,
    catalogues: Res<Assets<WorldScenarioDocumentAsset>>,
    documents: Res<Assets<UiDocumentAsset>>,
    mut lists: Query<
        (Entity, Ref<UiListPolicy>, &UiDocumentOwner),
        With<UiAuthoredToggleGroupSelectionPolicy>,
    >,
    roots: Query<&UiDocumentRoot>,
    world_choices: Query<(Entity, Ref<WorldChoice>, &GlobeMarker)>,
    mut rows: Query<(
        &UiListRow,
        Option<&WorldChoiceView>,
        &mut UiSelected,
        &mut UiPreviousSelection,
    )>,
    children: Query<&Children>,
    mut texts: Query<&mut Text>,
    selection: Res<ShellSelection>,
    active: Option<Res<LocalizationPrecedenceIndex>>,
    localizations: Res<Assets<LocalizationAsset>>,
) {
    if !active_catalogue.is_changed()
        && !catalogues.is_changed()
        && !documents.is_changed()
        && !localizations.is_changed()
        && !active.as_ref().is_some_and(|active| active.is_changed())
        && world_choices
            .iter()
            .all(|(_, choice, _)| !choice.is_changed())
        && lists.iter().all(|(_, policy, _)| !policy.is_changed())
    {
        return;
    }
    let Some(localization) = active
        .as_deref()
        .and_then(|sources| sources.borrow_loaded_localization_view(&localizations))
    else {
        return;
    };
    let Some(catalogue) = active_catalogue.get(&catalogues) else {
        return;
    };
    for (entity, policy, owner) in &mut lists {
        if policy.source == UiWidgetLiveCollectionSource::WorldChoices {
            let Some(document) = roots
                .get(owner.0)
                .ok()
                .and_then(|root| documents.get(&root.document))
            else {
                continue;
            };
            let Some(prototype_index) = authored_first_toggle_list_row_index(
                document,
                UiWidgetLiveCollectionSource::WorldChoicePrototype,
            ) else {
                continue;
            };
            let mut choices = world_choices
                .iter()
                .filter(|(_, choice, _)| Some(choice.mode) == selection.mode)
                .filter_map(|(source, choice, _)| {
                    catalogue.map(choice.map).map(|map| {
                        let name_key = openzt2_game_data::AssetId(map.name_key.0);
                        let label = localization
                            .find_plain_localized_text(name_key)
                            .unwrap_or("Unnamed map");
                        (source, label, map.catalogue_order)
                    })
                })
                .collect::<Vec<_>>();
            choices.sort_by(
                |(_, left_label, left_order), (_, right_label, right_order)| {
                    left_order
                        .cmp(right_order)
                        .then_with(|| left_label.cmp(right_label))
                },
            );
            let mut row_entities = vec![None; choices.len().min(usize::from(u16::MAX) + 1)];
            for row_entity in children.get(entity).into_iter().flatten().copied() {
                let Ok((row, ..)) = rows.get(row_entity) else {
                    continue;
                };
                if let Some(slot) = row_entities.get_mut(usize::from(row.index)) {
                    *slot = Some(row_entity);
                } else {
                    commands.entity(row_entity).despawn();
                }
            }
            for ((index, (source, label, _)), row_entity) in
                choices.into_iter().enumerate().zip(row_entities)
            {
                if let Some((row_entity, (_, current, mut selected, mut previous))) =
                    row_entity.and_then(|entity| rows.get_mut(entity).ok().map(|row| (entity, row)))
                {
                    if current.is_none_or(|current| current.0 != source) {
                        let next = world_choices.get(source).is_ok_and(|(_, choice, _)| {
                            selection.scenario == Some(choice.scenario)
                        });
                        commands.entity(row_entity).insert(WorldChoiceView(source));
                        selected.set_if_neq(UiSelected(next));
                        previous.synchronize_with_non_authored_selection_change(next);
                    }
                    replace_text_in_list_row_and_descendants(
                        row_entity, label, &children, &mut texts,
                    );
                    continue;
                }
                let row = commands
                    .spawn((
                        Name::new(format!("map choice {index}")),
                        bevy::ui_widgets::Button,
                        UiPointerInteraction::None,
                        UiFocusable {
                            order: index as u32,
                            enabled: true,
                        },
                        UiFocusPresentation::default(),
                        UiInteractionEnabled(true),
                        UiPreviousPointerInteraction::from_current_interaction(
                            UiPointerInteraction::None,
                        ),
                        UiPreviousPointerPressTimeSeconds::default(),
                        (
                            UiSelected(false),
                            UiPreviousSelection::from_current_selection(false),
                            UiAuthoredLayoutDisplay::from_authored_display(Display::Flex),
                            Visibility::Inherited,
                            Pickable::default(),
                        ),
                        *owner,
                        UiListRow {
                            index: index as u16,
                        },
                        (WorldChoiceView(source), ChildOf(entity)),
                    ))
                    .id();
                if !insert_authored_text_prototype(
                    &mut commands,
                    row,
                    owner.0,
                    document,
                    prototype_index,
                    localization,
                    label.to_owned(),
                ) {
                    commands.entity(row).despawn();
                    continue;
                }
            }
        }
    }
}
