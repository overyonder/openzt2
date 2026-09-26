use bevy::prelude::*;
use openzt2_game_data::{
    localization::LocalizationFormatArgument,
    ui_document::node_property_binding::{
        UiBooleanPropertyBindingSource, UiImagePropertyBindingSource,
        UiIntegerPropertyBindingSource, UiTextPropertyBindingSource,
    },
    world_definitions::catalogue_and_progression::catalogue_definition_types::{
        CatalogueEntry, CatalogueFilterFlags,
    },
    AssetId,
};

use crate::assets::localization::localization_asset_types::LocalizationAsset;
use crate::assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::economy::authored_economy_fact_hydration::find_authored_object_or_placeable_price;
use crate::plugins::economy::zoo_cash_types::UnlimitedZooCash;
use crate::plugins::economy::zoo_cash_types::ZooCash;
use crate::plugins::progression::adoption_and_content_availability_types::ScenarioContentAvailability;
use crate::plugins::progression::catalogue_entry_availability::catalogue_entry_is_available;
use crate::plugins::progression::unlock_types::UnlockedCatalogueDefinitionSet;
use crate::plugins::ui::authored_ui_focus_state::UiFocusable;
use crate::plugins::ui::authored_ui_image_content_binding::UiImageBinding;
use crate::plugins::ui::authored_ui_interaction_enabled_binding::UiEnabledBinding;
use crate::plugins::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_node_projection_components::UiValue;
use crate::plugins::ui::authored_ui_node_projection_components::UiValueBinding;
use crate::plugins::ui::authored_ui_node_projection_components::UiVisibleBinding;
use crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding;
use crate::plugins::world_spawn::selected_world_identity::SelectedWorldIdentity;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::catalogue_world_availability_context::resolve_catalogue_world_session_mode_and_scenario_availability;
use crate::plugins::information::{
    catalogue_types::{CatalogueFilter, CataloguePanel},
    projection::text_replacement_operations::replace_projected_ui_text_if_changed,
};

const CATALOGUE_PAGE_SIZE: usize = 12;

/// Visits one page in purchase order.
pub(super) fn visit_catalogue_page<'a>(
    catalog: WorldDefinitionsView<'a>,
    panel: &CataloguePanel,
    filter: &CatalogueFilter,
    mut is_unlocked: impl FnMut(usize, &CatalogueEntry) -> bool,
    mut is_affordable: impl FnMut(AssetId) -> bool,
    mut visit: impl FnMut(u16, usize, &'a CatalogueEntry),
) -> usize {
    let skip = usize::from(panel.page).saturating_mul(CATALOGUE_PAGE_SIZE);
    let mut eligible = 0usize;
    let mut emitted = 0usize;
    for (index, entry) in catalog.catalogue_in_authored_purchase_order() {
        if entry.category != panel.category {
            continue;
        }
        let definition = AssetId(entry.definition.0);
        let unlocked = is_unlocked(index, entry);
        if (!unlocked
            && entry
                .filters
                .contains_all(CatalogueFilterFlags::HIDDEN_UNTIL_UNLOCKED))
            || (filter.unlocked_only && !unlocked)
        {
            continue;
        }
        if filter.affordable_only && !is_affordable(definition) {
            continue;
        }
        if eligible >= skip && emitted < CATALOGUE_PAGE_SIZE {
            visit(emitted as u16, index, entry);
            emitted += 1;
        }
        eligible += 1;
        if emitted == CATALOGUE_PAGE_SIZE {
            break;
        }
    }
    emitted
}

/// Updates the catalogue page’s reusable rows.
pub(in crate::plugins::information) fn project_catalogue_page(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    cash: Res<ZooCash>,
    unlimited_cash: Option<Res<UnlimitedZooCash>>,
    mut previously_unlimited: Local<bool>,
    unlocks: Res<UnlockedCatalogueDefinitionSet>,
    worlds: Query<
        (
            Ref<SelectedWorldIdentity>,
            Option<Ref<ScenarioContentAvailability>>,
        ),
        With<WorldRoot>,
    >,
    panels: Query<(
        Entity,
        Ref<CataloguePanel>,
        Ref<CatalogueFilter>,
        Option<Ref<InheritedVisibility>>,
    )>,
    mut text_nodes: Query<(&UiDocumentOwner, &UiTextBinding, &mut Text)>,
    mut image_nodes: Query<(
        Entity,
        &UiDocumentOwner,
        &UiImageBinding,
        Option<&mut ImageNode>,
    )>,
    mut value_nodes: Query<(&UiDocumentOwner, &UiValueBinding, &mut UiValue)>,
    mut visible_nodes: Query<(&UiDocumentOwner, &UiVisibleBinding, &mut Visibility)>,
    mut enabled_nodes: Query<(
        &UiDocumentOwner,
        &UiEnabledBinding,
        &mut UiInteractionEnabled,
        Option<&mut UiFocusable>,
        &mut Pickable,
    )>,
) {
    let is_unlimited = unlimited_cash.is_some();
    let cash_policy_changed = is_unlimited != *previously_unlimited;
    let definitions_changed = definitions.is_changed();
    let localizations_changed = localizations.is_changed() || active_localization.is_changed();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(localization) = active_localization.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    let catalog = definitions;
    let (mode, scenario) = resolve_catalogue_world_session_mode_and_scenario_availability(&worlds);
    for (panel_entity, panel, filter, inherited_visibility) in &panels {
        if inherited_visibility
            .as_ref()
            .is_some_and(|visibility| !visibility.get())
        {
            continue;
        }
        if !panel.is_changed()
            && !filter.is_changed()
            && inherited_visibility
                .as_ref()
                .is_none_or(|visibility| !visibility.is_changed())
            && !cash.is_changed()
            && !cash_policy_changed
            && !unlocks.is_changed()
            && !definitions_changed
            && !localizations_changed
        {
            continue;
        }

        let mut rows: [Option<(usize, &CatalogueEntry)>; CATALOGUE_PAGE_SIZE] =
            [None; CATALOGUE_PAGE_SIZE];
        visit_catalogue_page(
            catalog,
            &panel,
            &filter,
            |index, entry| {
                catalogue_entry_is_available(catalog, index, entry, mode, scenario, &unlocks)
            },
            |definition| {
                is_unlimited
                    || find_authored_object_or_placeable_price(catalog, definition)
                        .is_some_and(|price| price.0 <= cash.0 .0)
            },
            |row, index, entry| {
                rows[usize::from(row)] = Some((index, entry));
            },
        );

        for (owner, binding, mut text) in &mut text_nodes {
            if owner.0 != panel_entity {
                continue;
            }
            match &binding.0 {
                UiTextPropertyBindingSource::CatalogueEntryName { row } => {
                    let value = catalogue_row(&rows, *row)
                        .and_then(|(_, entry)| {
                            localization.find_plain_localized_text(AssetId(entry.name_key.0))
                        })
                        .unwrap_or("");
                    replace_projected_ui_text_if_changed(&mut text.0, value);
                }
                UiTextPropertyBindingSource::CatalogueEntryPrice {
                    row,
                    format,
                    omit_fractional_currency_cents,
                } => {
                    text.0.clear();
                    if let Some(price) = catalogue_row(&rows, *row).and_then(|(_, entry)| {
                        find_authored_object_or_placeable_price(
                            catalog,
                            AssetId(entry.definition.0),
                        )
                    }) {
                        let mut formatted_amount = String::new();
                        let _ = localization.write_localized_currency_amount(
                            price.0,
                            !omit_fractional_currency_cents,
                            &mut formatted_amount,
                        );
                        let _ = localization.write_localized_text_with_format_arguments(
                            *format,
                            &[LocalizationFormatArgument::Text(&formatted_amount)],
                            &mut text.0,
                        );
                    }
                }
                _ => {}
            }
        }
        for (entity, owner, binding, image) in &mut image_nodes {
            if owner.0 != panel_entity {
                continue;
            }
            let UiImagePropertyBindingSource::CatalogueEntryIcon { row } = &binding.0 else {
                continue;
            };
            let handle = catalogue_row(&rows, *row)
                .and_then(|(_, entry)| definitions.texture_image(AssetId(entry.icon.0)));
            match (handle, image) {
                (Some(handle), Some(mut image)) => {
                    if image.image != handle {
                        image.image = handle;
                    }
                }
                (Some(handle), None) => {
                    commands.entity(entity).insert(ImageNode::new(handle));
                }
                (None, Some(mut image)) => {
                    image.image = Handle::default();
                }
                (None, None) => {}
            }
        }
        for (owner, binding, mut value) in &mut value_nodes {
            if owner.0 != panel_entity {
                continue;
            }
            value.0 = match &binding.0 {
                UiIntegerPropertyBindingSource::CatalogueEntryPriceCents { row } => {
                    catalogue_row(&rows, *row)
                        .and_then(|(_, entry)| {
                            find_authored_object_or_placeable_price(
                                catalog,
                                AssetId(entry.definition.0),
                            )
                        })
                        .map_or(0, |price| price.0)
                }
                UiIntegerPropertyBindingSource::CataloguePage => i64::from(panel.page),
                _ => continue,
            };
        }
        for (owner, binding, mut visibility) in &mut visible_nodes {
            if owner.0 != panel_entity {
                continue;
            }
            let shown = match &binding.0 {
                UiBooleanPropertyBindingSource::CatalogueRowVisible { row } => {
                    catalogue_row(&rows, *row).is_some()
                }
                UiBooleanPropertyBindingSource::CatalogueEntryUnlocked { row } => {
                    catalogue_row(&rows, *row).is_some_and(|(index, entry)| {
                        catalogue_entry_is_available(
                            catalog, index, entry, mode, scenario, &unlocks,
                        )
                    })
                }
                UiBooleanPropertyBindingSource::CatalogueEntryAffordable { row } => {
                    catalogue_row(&rows, *row).is_some_and(|(_, entry)| {
                        is_unlimited
                            || find_authored_object_or_placeable_price(
                                catalog,
                                AssetId(entry.definition.0),
                            )
                            .is_some_and(|price| price.0 <= cash.0 .0)
                    })
                }
                _ => continue,
            };
            *visibility = if shown {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
        for (owner, binding, mut interaction_enabled, focusable, mut pickable) in &mut enabled_nodes
        {
            if owner.0 != panel_entity {
                continue;
            }
            let enabled = match &binding.0 {
                UiBooleanPropertyBindingSource::CatalogueRowVisible { row } => {
                    catalogue_row(&rows, *row).is_some()
                }
                UiBooleanPropertyBindingSource::CatalogueEntryUnlocked { row } => {
                    catalogue_row(&rows, *row).is_some_and(|(index, entry)| {
                        catalogue_entry_is_available(
                            catalog, index, entry, mode, scenario, &unlocks,
                        )
                    })
                }
                UiBooleanPropertyBindingSource::CatalogueEntryAffordable { row } => {
                    catalogue_row(&rows, *row).is_some_and(|(_, entry)| {
                        is_unlimited
                            || find_authored_object_or_placeable_price(
                                catalog,
                                AssetId(entry.definition.0),
                            )
                            .is_some_and(|price| price.0 <= cash.0 .0)
                    })
                }
                _ => continue,
            };
            interaction_enabled.0 = enabled;
            if let Some(mut focusable) = focusable {
                focusable.enabled = enabled;
            }
            *pickable = if enabled {
                Pickable::default()
            } else {
                Pickable::IGNORE
            };
        }
    }
    *previously_unlimited = is_unlimited;
}

fn catalogue_row<'a>(
    rows: &[Option<(usize, &'a CatalogueEntry)>; CATALOGUE_PAGE_SIZE],
    row: u16,
) -> Option<(usize, &'a CatalogueEntry)> {
    rows.get(usize::from(row)).copied().flatten()
}
