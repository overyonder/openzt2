use bevy::prelude::*;
use openzt2_game_data::{
    localization::LocalizationFormatArgument,
    ui_document::widget::UiWidgetRecord,
    ui_document::{
        document::UiDocumentRole,
        node_property_binding::{
            UiBooleanPropertyBindingSource, UiImagePropertyBindingSource,
            UiIntegerPropertyBindingSource, UiTextPropertyBindingSource,
        },
    },
    world_definitions::catalogue_and_progression::catalogue_definition_types::CatalogueCategory,
    AssetId,
};

use crate::assets::localization::localization_asset_types::LocalizationAsset;
use crate::assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use crate::assets::species::species_asset_types::SpeciesAsset;
use crate::assets::species::species_asset_types::SpeciesAssets;
use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_lifecycle::animal_adoption_contracts::BeginAnimalAdoptionPlacement;
use crate::plugins::economy::authored_economy_fact_hydration::find_authored_object_or_placeable_price;
use crate::plugins::ui::authored_multi_icon_presentation::UiMultiIconPolicy;
use crate::plugins::ui::authored_ui_image_content_binding::UiImageBinding;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;
use crate::plugins::ui::authored_ui_node_projection_components::UiNodeId;
use crate::plugins::ui::authored_ui_node_projection_components::UiValue;
use crate::plugins::ui::authored_ui_node_projection_components::UiValueBinding;
use crate::plugins::ui::authored_ui_node_projection_components::UiVisibleBinding;
use crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding;
use crate::plugins::ui::ui_document_lifecycle_contracts::ShowUiRole;

use super::super::{
    catalogue_types::{CatalogueDetails, PurchaseChoice, SelectedCatalogueEntry},
    projection::{
        animal_pickup_release_information_projection::authored_multi_icon_key_for_conservation_status,
        text_replacement_operations::replace_projected_ui_text_if_changed,
    },
};

/// Selects the last catalogue choice produced during this update and opens its
/// authored purchase-information document under the active HUD owner.
pub(in crate::plugins::information) fn select_catalogue_entry_and_open_purchase_information_document(
    mut purchase_choices: MessageReader<PurchaseChoice>,
    mut adoption_placement_requests: MessageReader<BeginAnimalAdoptionPlacement>,
    mut selected_catalogue_entry: ResMut<SelectedCatalogueEntry>,
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    ui_document_roots: Query<(&UiDocumentRoot, &ChildOf)>,
    mut show_ui_role_requests: MessageWriter<ShowUiRole>,
) {
    let selected_definition = purchase_choices
        .read()
        .map(|choice| choice.definition)
        .chain(
            adoption_placement_requests
                .read()
                .map(|choice| choice.species),
        )
        .last();
    let Some(selected_definition) = selected_definition else {
        return;
    };
    selected_catalogue_entry.0 = Some(selected_definition);

    let Some(hud_lifecycle_owner) = ui_document_roots.iter().find_map(|(root, parent)| {
        ui_document_assets
            .get(&root.document)
            .is_some_and(|document| {
                matches!(
                    &document.canonical_ui_document().role,
                    UiDocumentRole::InGameHud
                )
            })
            .then_some(parent.parent())
    }) else {
        return;
    };
    show_ui_role_requests.write(ShowUiRole {
        role: UiDocumentRole::PurchaseCatalogue,
        owner: hud_lifecycle_owner,
    });
}

pub(in crate::plugins::information) fn project_selected_catalogue_entry_to_purchase_information_document(
    mut commands: Commands,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    species_assets: Res<Assets<SpeciesAsset>>,
    active_species: Res<SpeciesAssets>,
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    ui_document_roots: Query<&UiDocumentRoot>,
    selected_catalogue_entry: Res<SelectedCatalogueEntry>,
    mut catalogue_detail_panels: Query<(&UiDocumentOwner, &mut Visibility, Ref<CatalogueDetails>)>,
    mut visibility_nodes: Query<
        (&UiDocumentOwner, &UiVisibleBinding, &mut Visibility),
        Without<CatalogueDetails>,
    >,
    mut text_nodes: Query<(&UiDocumentOwner, &UiTextBinding, &mut Text)>,
    mut image_nodes: Query<(
        Entity,
        &UiDocumentOwner,
        &UiImageBinding,
        Option<&mut ImageNode>,
    )>,
    mut value_nodes: Query<(
        &UiNodeId,
        &UiDocumentOwner,
        &UiValueBinding,
        &UiMultiIconPolicy,
        &mut UiValue,
    )>,
) {
    let world_definitions_changed = world_definition_assets.is_changed();
    let localization_changed = localization_assets.is_changed() || active_localization.is_changed();
    let species_changed = species_assets.is_changed() || active_species.is_changed();
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(localization) =
        active_localization.borrow_loaded_localization_view(&localization_assets)
    else {
        return;
    };
    let species = active_species.get(&species_assets);

    for (document_owner, mut visibility, catalogue_details) in &mut catalogue_detail_panels {
        let Some(selected_definition) = selected_catalogue_entry.0 else {
            if *visibility != Visibility::Hidden {
                *visibility = Visibility::Hidden;
            }
            continue;
        };
        if *visibility != Visibility::Inherited {
            *visibility = Visibility::Inherited;
        }
        if !selected_catalogue_entry.is_changed()
            && !world_definitions_changed
            && !localization_changed
            && !species_changed
            && !catalogue_details.is_added()
        {
            continue;
        }
        let Some(catalogue_entry) = world_definitions
            .catalogue()
            .find(|entry| AssetId(entry.definition.0) == selected_definition)
        else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let authored_object = world_definitions.find_object(selected_definition);
        let ui_document = ui_document_roots
            .get(document_owner.0)
            .ok()
            .and_then(|root| ui_document_assets.get(&root.document));
        let conservation_status_key = species
            .and_then(|species| species.find(selected_definition))
            .and_then(|species| {
                authored_multi_icon_key_for_conservation_status(species.conservation)
            });

        for (node_id, candidate_document_owner, binding, _, mut value) in &mut value_nodes {
            if candidate_document_owner.0 != document_owner.0
                || !matches!(
                    binding.0,
                    UiIntegerPropertyBindingSource::CatalogueEntryConservationStatus
                )
            {
                continue;
            }
            value.0 = ui_document
                .and_then(|document| {
                    let UiWidgetRecord::MultiIcon { entries } = &document
                        .canonical_ui_document()
                        .nodes
                        .get(node_id.index as usize)?
                        .widget
                    else {
                        return None;
                    };
                    entries
                        .iter()
                        .position(|entry| Some(AssetId(entry.key.0)) == conservation_status_key)
                })
                .map_or(-1, |index| index as i64);
        }

        for (candidate_document_owner, binding, mut visibility) in &mut visibility_nodes {
            if candidate_document_owner.0 != document_owner.0 {
                continue;
            }
            let visible = match binding.0 {
                UiBooleanPropertyBindingSource::CatalogueSelectedEntryHasMonthlyUpkeep => {
                    authored_object.is_some_and(|object| object.upkeep_cents_per_month > 0)
                }
                UiBooleanPropertyBindingSource::CatalogueSelectedEntryHasBiomeAndLocation => {
                    catalogue_entry.category != CatalogueCategory::Animals
                        && authored_object.is_some_and(|object| {
                            !object.biomes.is_empty() && object.location != AssetId::default()
                        })
                }
                UiBooleanPropertyBindingSource::CatalogueSelectedEntryIsAnimal => {
                    catalogue_entry.category == CatalogueCategory::Animals
                }
                UiBooleanPropertyBindingSource::CatalogueSelectedEntryPlacementCanRotate => {
                    !matches!(
                        catalogue_entry.category,
                        CatalogueCategory::Fences | CatalogueCategory::Paths
                    )
                }
                UiBooleanPropertyBindingSource::CatalogueSelectedEntrySellsItems => {
                    world_definitions
                        .find_facility_by_object(selected_definition)
                        .is_some_and(|facility| facility.inventory_units_per_service != 0)
                }
                _ => continue,
            };
            let projected_visibility = if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *visibility != projected_visibility {
                *visibility = projected_visibility;
            }
        }

        for (candidate_document_owner, binding, mut text) in &mut text_nodes {
            if candidate_document_owner.0 != document_owner.0 {
                continue;
            }
            match &binding.0 {
                UiTextPropertyBindingSource::CatalogueEntryName { .. } => {
                    let selected_name = localization
                        .find_plain_localized_text(AssetId(catalogue_entry.name_key.0))
                        .unwrap_or("");
                    replace_projected_ui_text_if_changed(&mut text.0, selected_name);
                }
                UiTextPropertyBindingSource::CatalogueEntryPrice {
                    format,
                    omit_fractional_currency_cents,
                    ..
                } => {
                    text.0.clear();
                    if let Some(price) = find_authored_object_or_placeable_price(
                        world_definitions,
                        selected_definition,
                    ) {
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
                UiTextPropertyBindingSource::CatalogueEntryResearchPrice {
                    format,
                    omit_fractional_currency_cents,
                } => {
                    text.0.clear();
                    if let Some(research) = world_definitions
                        .research()
                        .find(|research| research.unlocks.contains(&selected_definition))
                    {
                        let mut amount = String::new();
                        let _ = localization.write_localized_currency_amount(
                            research.cost_cents,
                            !omit_fractional_currency_cents,
                            &mut amount,
                        );
                        let _ = localization.write_localized_text_with_format_arguments(
                            *format,
                            &[LocalizationFormatArgument::Text(&amount)],
                            &mut text.0,
                        );
                    }
                }
                UiTextPropertyBindingSource::CatalogueEntryUpkeep {
                    format,
                    omit_fractional_currency_cents,
                    ..
                } => {
                    text.0.clear();
                    let upkeep_cents_per_month = authored_object
                        .map(|object| i64::from(object.upkeep_cents_per_month))
                        .unwrap_or_default();
                    let mut formatted_amount = String::new();
                    let _ = localization.write_localized_currency_amount(
                        upkeep_cents_per_month,
                        !omit_fractional_currency_cents,
                        &mut formatted_amount,
                    );
                    let _ = localization.write_localized_text_with_format_arguments(
                        *format,
                        &[LocalizationFormatArgument::Text(&formatted_amount)],
                        &mut text.0,
                    );
                }
                _ => {}
            }
        }

        for (image_node_entity, candidate_document_owner, binding, image) in &mut image_nodes {
            if candidate_document_owner.0 != document_owner.0 {
                continue;
            }
            let Some(texture_identifier) = (match &binding.0 {
                UiImagePropertyBindingSource::CatalogueEntryIcon { .. } => {
                    Some(Some(AssetId(catalogue_entry.icon.0)))
                }
                UiImagePropertyBindingSource::CatalogueEntryBiomeIcon => Some(
                    authored_object
                        .and_then(|object| object.biomes.first())
                        .and_then(|biome| world_definitions.find_biome(*biome))
                        .map(|biome| AssetId(biome.icon.0)),
                ),
                UiImagePropertyBindingSource::CatalogueEntryLocationIcon => Some(
                    authored_object
                        .and_then(|object| {
                            world_definitions.find_location(AssetId(object.location.0))
                        })
                        .map(|location| AssetId(location.icon.0)),
                ),
                _ => None,
            }) else {
                continue;
            };
            let texture_image = texture_identifier
                .and_then(|identifier| world_definitions.texture_image(identifier));
            match (texture_image, image) {
                (Some(texture_image), Some(mut image)) => {
                    if image.image != texture_image {
                        image.image = texture_image;
                    }
                }
                (Some(texture_image), None) => {
                    commands
                        .entity(image_node_entity)
                        .insert(ImageNode::new(texture_image));
                }
                (None, Some(mut image)) => {
                    image.image = Handle::default();
                }
                (None, None) => {}
            }
        }
    }
}

/// Applies the native `ZTBuyInfoPanel` dynamic row stack after the selected
/// object's optional groups have received their visibility.
pub(in crate::plugins::information) fn position_visible_purchase_information_rows_from_authored_dynamic_start(
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    ui_document_roots: Query<&UiDocumentRoot>,
    catalogue_detail_panels: Query<&UiDocumentOwner, With<CatalogueDetails>>,
    mut projected_nodes: Query<(&UiNodeId, &UiDocumentOwner, &Visibility, &mut Node)>,
) {
    for document_owner in &catalogue_detail_panels {
        let Some(document) = ui_document_roots
            .get(document_owner.0)
            .ok()
            .and_then(|root| ui_document_assets.get(&root.document))
        else {
            continue;
        };
        let canonical_document = document.canonical_ui_document();
        let Some((dynamic_x, mut dynamic_y)) =
            projected_nodes
                .iter()
                .find_map(|(node_id, candidate_document_owner, _, node)| {
                    (candidate_document_owner.0 == document_owner.0
                        && canonical_document
                            .nodes
                            .get(node_id.index as usize)
                            .is_some_and(|record| record.name.eq_ignore_ascii_case("cost")))
                    .then_some((node.left, node.top))
                })
        else {
            continue;
        };
        for dynamic_row_name in [
            "cost",
            "gender",
            "biome",
            "biome_location_endangerment",
            "Items Sold",
        ] {
            for (node_id, candidate_document_owner, visibility, mut node) in &mut projected_nodes {
                if candidate_document_owner.0 != document_owner.0
                    || *visibility == Visibility::Hidden
                    || !canonical_document
                        .nodes
                        .get(node_id.index as usize)
                        .is_some_and(|record| record.name.eq_ignore_ascii_case(dynamic_row_name))
                {
                    continue;
                }
                if node.left != dynamic_x {
                    node.left = dynamic_x;
                }
                if node.top != dynamic_y {
                    node.top = dynamic_y;
                }
                if let (Val::Px(y), Val::Px(height)) = (dynamic_y, node.height) {
                    dynamic_y = Val::Px(y + height);
                }
                break;
            }
        }
    }
}
