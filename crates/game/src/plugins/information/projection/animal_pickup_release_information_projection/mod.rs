use bevy::prelude::*;
use openzt2_game_data::{
    species::ConservationStatus,
    ui_document::{node_property_binding::*, widget::UiWidgetRecord},
    AssetId,
};

use crate::{
    assets::{
        localization::{
            localization_asset_types::LocalizationAsset,
            localization_precedence_index::LocalizationPrecedenceIndex,
        },
        species::species_asset_types::{SpeciesAsset, SpeciesAssets},
        ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    },
    plugins::{
        animal_health::types::{Dead, Rampaging},
        animal_lifecycle::types::{
            Adoptable, Animal, CratingRestricted, ReleaseRestricted, SpeciesHandle,
        },
        ui::{
            authored_multi_icon_presentation::UiMultiIconPolicy,
            authored_ui_focus_state::UiFocusable,
            authored_ui_interaction_enabled_binding::UiEnabledBinding,
            authored_ui_interaction_enabled_state::UiInteractionEnabled,
            authored_ui_node_projection_components::UiDocumentOwner,
            authored_ui_node_projection_components::UiDocumentRoot,
            authored_ui_node_projection_components::UiNodeId,
            authored_ui_node_projection_components::UiValue,
            authored_ui_node_projection_components::UiValueBinding,
            authored_ui_node_projection_components::UiVisibleBinding,
            authored_ui_text_content_binding::UiTextBinding,
        },
        world_spawn::world_entity_crating::WorldEntityIsCrated,
    },
};

use super::super::entity_selection_types::InfoPanel;
use super::text_replacement_operations::replace_projected_ui_text_if_changed;

pub(in crate::plugins::information) fn authored_multi_icon_key_for_conservation_status(
    conservation_status: ConservationStatus,
) -> Option<AssetId> {
    match conservation_status {
        ConservationStatus::Unspecified => None,
        ConservationStatus::LowRisk => Some(AssetId::from_key("LowRisk")),
        ConservationStatus::Vulnerable => Some(AssetId::from_key("Vulnerable")),
        ConservationStatus::Endangered => Some(AssetId::from_key("Endangered")),
        ConservationStatus::Critical => Some(AssetId::from_key("Critical")),
        ConservationStatus::Extinct => Some(AssetId::from_key("Extinct")),
    }
}

pub(in crate::plugins::information) fn project_selected_animal_pickup_release_and_conservation_controls(
    species_assets: Res<Assets<SpeciesAsset>>,
    active_species: Res<SpeciesAssets>,
    ui_documents: Res<Assets<UiDocumentAsset>>,
    ui_document_roots: Query<&UiDocumentRoot>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    information_panels: Query<(Entity, &InfoPanel, &InheritedVisibility)>,
    animals: Query<
        (
            &SpeciesHandle,
            Has<Adoptable>,
            Has<ReleaseRestricted>,
            Has<CratingRestricted>,
            Has<WorldEntityIsCrated>,
            Has<Rampaging>,
            Has<Dead>,
        ),
        With<Animal>,
    >,
    mut visibility_nodes: Query<(&UiDocumentOwner, &UiVisibleBinding, &mut Visibility)>,
    mut interaction_nodes: Query<(
        &UiDocumentOwner,
        &UiEnabledBinding,
        &mut UiInteractionEnabled,
        Option<&mut UiFocusable>,
        &mut Pickable,
    )>,
    mut conservation_icon_nodes: Query<(
        &UiNodeId,
        &UiDocumentOwner,
        &UiValueBinding,
        &UiMultiIconPolicy,
        &mut UiValue,
    )>,
    mut conservation_text_nodes: Query<(&UiDocumentOwner, &UiTextBinding, &mut Text)>,
) {
    let Some(species_index) = active_species.get(&species_assets) else {
        return;
    };
    for (panel_entity, information_panel, inherited_visibility) in &information_panels {
        if !inherited_visibility.get() {
            continue;
        }
        let Ok((
            species_handle,
            is_adoptable,
            release_is_restricted,
            crating_is_restricted,
            is_crated,
            is_rampaging,
            is_dead,
        )) = animals.get(information_panel.subject)
        else {
            continue;
        };
        let conservation_status = species_index
            .find(species_handle.species)
            .map(|species| species.conservation)
            .unwrap_or(ConservationStatus::Unspecified);
        let conservation_status_key =
            authored_multi_icon_key_for_conservation_status(conservation_status);
        let ui_document = ui_document_roots
            .get(panel_entity)
            .ok()
            .and_then(|document_root| ui_documents.get(&document_root.document));
        let mut conservation_localization_key = None;
        for (node_id, document_owner, value_binding, _, mut value) in &mut conservation_icon_nodes {
            if document_owner.0 != panel_entity
                || !matches!(
                    value_binding.0,
                    UiIntegerPropertyBindingSource::AnimalConservationStatus
                )
            {
                continue;
            }
            let Some((entry_index, icon_entry)) = ui_document.and_then(|ui_document| {
                let UiWidgetRecord::MultiIcon { entries } = &ui_document
                    .canonical_ui_document()
                    .nodes
                    .get(node_id.index as usize)?
                    .widget
                else {
                    return None;
                };
                entries.iter().enumerate().find(|(_, icon_entry)| {
                    Some(AssetId(icon_entry.key.0)) == conservation_status_key
                })
            }) else {
                value.0 = -1;
                continue;
            };
            value.0 = entry_index as i64;
            conservation_localization_key = Some(AssetId(icon_entry.localization_key.0));
        }
        let conservation_label = active_localization
            .borrow_loaded_localization_view(&localization_assets)
            .zip(conservation_localization_key)
            .and_then(|(active_localization, localization_key)| {
                active_localization.find_plain_localized_text(localization_key)
            })
            .unwrap_or_default();
        for (document_owner, text_binding, mut text) in &mut conservation_text_nodes {
            if document_owner.0 == panel_entity
                && matches!(
                    text_binding.0,
                    UiTextPropertyBindingSource::AnimalConservationStatus
                )
            {
                replace_projected_ui_text_if_changed(&mut text.0, conservation_label);
            }
        }
        let pickup_should_be_visible = is_adoptable && !is_dead;
        let release_should_be_visible = !release_is_restricted && !is_dead;
        let pickup_should_be_enabled =
            pickup_should_be_visible && !crating_is_restricted && !is_rampaging;
        for (document_owner, visibility_binding, mut visibility) in &mut visibility_nodes {
            if document_owner.0 != panel_entity {
                continue;
            }
            let should_be_visible = match &visibility_binding.0 {
                UiBooleanPropertyBindingSource::AnimalPickupVisible => pickup_should_be_visible,
                UiBooleanPropertyBindingSource::AnimalReleaseVisible => release_should_be_visible,
                UiBooleanPropertyBindingSource::SelectedEntityCrated => is_crated,
                _ => continue,
            };
            *visibility = if should_be_visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
        for (document_owner, enabled_binding, mut interaction_enabled, focusable, mut pickable) in
            &mut interaction_nodes
        {
            if document_owner.0 != panel_entity
                || !matches!(
                    &enabled_binding.0,
                    UiBooleanPropertyBindingSource::AnimalPickupEnabled
                )
            {
                continue;
            }
            interaction_enabled.0 = pickup_should_be_enabled;
            if let Some(mut focusable) = focusable {
                focusable.enabled = pickup_should_be_enabled;
            }
            *pickable = if pickup_should_be_enabled {
                Pickable::default()
            } else {
                Pickable::IGNORE
            };
        }
    }
}
