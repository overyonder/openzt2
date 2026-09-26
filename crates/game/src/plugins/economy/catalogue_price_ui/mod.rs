use super::{
    authored_economy_fact_hydration::find_authored_object_or_placeable_price, money_types::Money,
};
use crate::assets::localization::localization_asset_types::LocalizationAsset;
use crate::assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::information::catalogue_types::CatalogueRowEntry;
use crate::plugins::ui::authored_ui_node_projection_components::UiValue;
use crate::plugins::ui::authored_ui_node_projection_components::UiValueBinding;
use crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding;
use bevy::prelude::*;
use openzt2_game_data::{
    localization::LocalizationFormatArgument,
    ui_document::node_property_binding::{
        UiIntegerPropertyBindingSource, UiTextPropertyBindingSource,
    },
    AssetId,
};

/// Projects the loaded price of a dynamic catalogue row into its ordinary
/// Bevy UI descendants. The row identity selects the canonical world definition.
pub(super) fn project_authored_catalogue_entry_prices_into_dynamic_ui_rows(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    rows: Query<Ref<CatalogueRowEntry>>,
    parents: Query<&ChildOf>,
    mut text_fields: Query<(Entity, Ref<UiTextBinding>, &mut Text)>,
    mut value_fields: Query<(Entity, Ref<UiValueBinding>, &mut UiValue)>,
) {
    let definitions_changed = definitions.is_changed();
    let localizations_changed = localizations.is_changed() || active_localization.is_changed();
    let Some(world_definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let localization = active_localization.borrow_loaded_localization_view(&localizations);

    for (entity, binding, mut text) in &mut text_fields {
        let UiTextPropertyBindingSource::CatalogueEntryPrice {
            format,
            omit_fractional_currency_cents,
            ..
        } = &binding.0
        else {
            continue;
        };
        let Some((definition, row_changed)) =
            find_containing_catalogue_row_definition(entity, &parents, &rows)
        else {
            continue;
        };
        if !definitions_changed && !localizations_changed && !binding.is_added() && !row_changed {
            continue;
        }

        text.0.clear();
        // The original callable always emitted a float initialized to zero
        // when its current-item lookup missed.
        let price = find_authored_object_or_placeable_price(world_definitions, definition)
            .unwrap_or(Money::ZERO);
        if let Some(localization) = localization {
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

    for (entity, binding, mut value) in &mut value_fields {
        if !matches!(
            &binding.0,
            UiIntegerPropertyBindingSource::CatalogueEntryPriceCents { .. }
        ) {
            continue;
        }
        let Some((definition, row_changed)) =
            find_containing_catalogue_row_definition(entity, &parents, &rows)
        else {
            continue;
        };
        if !definitions_changed && !binding.is_added() && !row_changed {
            continue;
        }
        value.0 = find_authored_object_or_placeable_price(world_definitions, definition)
            .map(|price| price.0)
            .unwrap_or_default();
    }
}

fn find_containing_catalogue_row_definition(
    mut entity: Entity,
    parents: &Query<&ChildOf>,
    rows: &Query<Ref<CatalogueRowEntry>>,
) -> Option<(AssetId, bool)> {
    // Native UI validation limits document depth. This cap also bounds corrupt
    // hierarchy traversal without allocating scratch storage.
    for _ in 0..64 {
        if let Ok(row) = rows.get(entity) {
            return Some((row.definition, row.is_changed()));
        }
        entity = parents.get(entity).ok()?.parent();
    }
    None
}
