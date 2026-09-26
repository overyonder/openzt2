use super::facility_economy_types::CurrentSellQuote;
use crate::{
    assets::{
        localization::{
            localization_asset_types::LocalizationAsset,
            localization_precedence_index::LocalizationPrecedenceIndex,
        },
        world_definitions::{
            world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
            world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
        },
    },
    plugins::{
        information::entity_selection_types::SelectedEntity,
        ui::authored_ui_text_content_binding::UiTextBinding,
        world_spawn::world_membership_types::DefinitionId,
    },
};
use bevy::prelude::*;
use openzt2_game_data::{
    localization::LocalizationFormatArgument,
    ui_document::node_property_binding::UiTextPropertyBindingSource, AssetId,
};

pub(super) fn project_selected_object_sale_confirmation(
    selected: Res<SelectedEntity>,
    objects: Query<(&DefinitionId, Ref<CurrentSellQuote>)>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    mut fields: Query<(Ref<UiTextBinding>, &mut Text)>,
    mut removed_quotes: RemovedComponents<CurrentSellQuote>,
) {
    let quote_removed = removed_quotes.read().count() != 0;
    let object = selected.0.and_then(|entity| objects.get(entity).ok());
    let localization = active_localization.borrow_loaded_localization_view(&localizations);
    let definition = object.as_ref().and_then(|(definition, _)| {
        active_definitions
            .get(&definitions)?
            .find_object(definition.0)
    });
    for (binding, mut text) in &mut fields {
        if !matches!(
            &binding.0,
            UiTextPropertyBindingSource::SelectedEntitySaleConfirmation
                | UiTextPropertyBindingSource::SelectedEntitySaleRefund { .. }
        ) {
            continue;
        }
        if !quote_removed
            && !binding.is_added()
            && !selected.is_changed()
            && !definitions.is_changed()
            && !active_definitions.is_changed()
            && !localizations.is_changed()
            && !active_localization.is_changed()
            && !object.as_ref().is_some_and(|(_, quote)| quote.is_changed())
        {
            continue;
        }
        text.0.clear();
        let Some((_, quote)) = &object else {
            continue;
        };
        let Some(localization) = localization else {
            continue;
        };
        match &binding.0 {
            UiTextPropertyBindingSource::SelectedEntitySaleConfirmation => {
                let name = definition.and_then(|definition| {
                    localization.find_plain_localized_text(definition.name_key)
                });
                if let Some(name) = name {
                    let _ = localization.write_localized_text_with_format_arguments(
                        AssetId::from_key("confirm:sell_selected_item"),
                        &[LocalizationFormatArgument::Text(name)],
                        &mut text.0,
                    );
                }
            }
            UiTextPropertyBindingSource::SelectedEntitySaleRefund {
                format,
                omit_fractional_currency_cents,
            } => {
                let mut amount = String::new();
                let _ = localization.write_localized_currency_amount(
                    quote.0 .0,
                    !omit_fractional_currency_cents,
                    &mut amount,
                );
                let _ = localization.write_localized_text_with_format_arguments(
                    *format,
                    &[LocalizationFormatArgument::Text(&amount)],
                    &mut text.0,
                );
            }
            _ => {}
        }
    }
}
