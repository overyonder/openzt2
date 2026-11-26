use bevy::picking::hover::DirectlyHovered;
use bevy::prelude::*;
use openzt2_game_data::ui_document::widget_live_collection::UiWidgetLiveCollectionSource;
use openzt2_ui::authored_pointer_events::LastAuthoredPrimaryPress;

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use openzt2_interface_state::ui::authored_ui_focus_state::UiFocusable;
use openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;
use openzt2_ui::authored_toggle_selection_transitions::UiAuthoredToggleGroupSelectionPolicy;
use openzt2_ui::authored_ui_layout_participation::UiAuthoredLayoutDisplay;
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
    lists: Query<
        (Entity, Ref<UiListPolicy>, &UiDocumentOwner),
        With<UiAuthoredToggleGroupSelectionPolicy>,
    >,
    roots: Query<(&UiDocumentRoot, &ChildOf)>,
    world_choices: Query<(Ref<WorldChoice>, &GlobeMarker)>,
    mut removed_choices: RemovedComponents<WorldChoice>,
    mut rows: Query<(&UiListRow, Option<&WorldChoiceView>, &mut UiSelected)>,
    children: Query<&Children>,
    mut texts: Query<&mut Text>,
    selection: Res<ShellSelection>,
    active: Option<Res<LocalizationPrecedenceIndex>>,
    localizations: Res<Assets<LocalizationAsset>>,
) {
    let choice_removed = removed_choices.read().count() != 0;
    if !choice_removed
        && !active_catalogue.is_changed()
        && !catalogues.is_changed()
        && !documents.is_changed()
        && !localizations.is_changed()
        && !active.as_ref().is_some_and(|active| active.is_changed())
        && world_choices.iter().all(|(choice, _)| !choice.is_changed())
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
    for (entity, policy, owner) in &lists {
        if policy.source == UiWidgetLiveCollectionSource::WorldChoices {
            let Ok((root, screen)) = roots.get(owner.0) else {
                continue;
            };
            let Some(document) = documents.get(&root.document) else {
                continue;
            };
            let Some(prototype_index) = authored_first_toggle_list_row_index(
                document,
                UiWidgetLiveCollectionSource::WorldChoicePrototype,
            ) else {
                continue;
            };
            let mut choices = children
                .get(screen.parent())
                .into_iter()
                .flat_map(|children| children.iter())
                .filter_map(|source| {
                    let (choice, _) = world_choices.get(source).ok()?;
                    if Some(choice.mode) != selection.mode {
                        return None;
                    }
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
                if let Some((row_entity, (_, current, mut selected))) =
                    row_entity.and_then(|entity| rows.get_mut(entity).ok().map(|row| (entity, row)))
                {
                    if current.is_none_or(|current| current.0 != source) {
                        let next = world_choices
                            .get(source)
                            .is_ok_and(|(choice, _)| selection.scenario == Some(choice.scenario));
                        commands.entity(row_entity).insert(WorldChoiceView(source));
                        selected.set_if_neq(UiSelected(next));
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
                        UiFocusable,
                        bevy::input_focus::tab_navigation::TabIndex(
                            i32::try_from(index).unwrap_or(i32::MAX),
                        ),
                        UiInteractionEnabled(true),
                        DirectlyHovered::default(),
                        LastAuthoredPrimaryPress::default(),
                        (
                            UiSelected(world_choices.get(source).is_ok_and(|(choice, _)| {
                                selection.scenario == Some(choice.scenario)
                            })),
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
