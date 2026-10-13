//! Keeper recommendations retain their animal when a supply is selected.

use openzt2_interface_state::information::catalogue_types::SelectedCareAnimal;
use openzt2_interface_state::information::entity_selection_types::SelectionChanged;
use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_lifecycle::animal_adoption_contracts::BeginAnimalAdoptionPlacement;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_ui::authored_ui_image_content_binding::UiImageBinding;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;
use bevy::prelude::*;
use openzt2_game_data::{
    localization::LocalizationFormatArgument,
    ui_document::{
        document::UiDocumentRole,
        node_property_binding::{UiImagePropertyBindingSource, UiTextPropertyBindingSource},
    },
    AssetId,
};

pub(crate) fn clear_care_animal(mut selected: ResMut<SelectedCareAnimal>) {
    selected.set_if_neq(SelectedCareAnimal(None));
}

pub(crate) fn retain_selected_animal_for_care_catalogue(
    mut selections: MessageReader<SelectionChanged>,
    mut adoptions: MessageReader<BeginAnimalAdoptionPlacement>,
    animals: Query<&SpeciesHandle>,
    mut selected: ResMut<SelectedCareAnimal>,
) {
    let animal = selections
        .read()
        .filter_map(|change| {
            animals
                .get(change.current?)
                .ok()
                .map(|species| species.species)
        })
        .chain(adoptions.read().map(|adoption| adoption.species))
        .last();
    if let Some(animal) = animal {
        selected.set_if_neq(SelectedCareAnimal(Some(animal)));
    }
}

pub(crate) fn project_care_animal_name_and_icon(
    mut commands: Commands,
    selected: Res<SelectedCareAnimal>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    mut names: Query<(&UiDocumentOwner, Ref<UiTextBinding>, &mut Text)>,
    mut icons: Query<(
        Entity,
        &UiDocumentOwner,
        Ref<UiImageBinding>,
        Option<&mut ImageNode>,
    )>,
) {
    let Some(definition) = selected.0 else {
        return;
    };
    let Some(definitions_view) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(object) = definitions_view.find_object(definition) else {
        return;
    };
    let Some(localization) =
        active_localization.borrow_loaded_localization_view(&localization_assets)
    else {
        return;
    };
    let is_care_document = |owner: &UiDocumentOwner| {
        roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
            .is_some_and(|document| {
                document.canonical_ui_document().role == UiDocumentRole::AnimalCareCatalogue
            })
    };
    let changed = selected.is_changed()
        || definitions.is_changed()
        || active_definitions.is_changed()
        || localization_assets.is_changed()
        || active_localization.is_changed();
    for (owner, binding, mut text) in &mut names {
        if !(changed || binding.is_added())
            || !is_care_document(owner)
            || !matches!(
                binding.0,
                UiTextPropertyBindingSource::CatalogueEntryName { .. }
            )
        {
            continue;
        }
        let name = localization
            .find_plain_localized_text(object.name_key)
            .unwrap_or("");
        let mut formatted = String::new();
        let _ = localization.write_localized_text_with_format_arguments(
            AssetId::from_key("sort:entity_name_format"),
            &[LocalizationFormatArgument::Text(name)],
            &mut formatted,
        );
        if text.0 != formatted {
            text.0 = formatted;
        }
    }
    for (entity, owner, binding, image) in &mut icons {
        if !(changed || binding.is_added())
            || !is_care_document(owner)
            || !matches!(
                binding.0,
                UiImagePropertyBindingSource::CatalogueEntryIcon { .. }
            )
        {
            continue;
        }
        if let Some(texture) = definitions_view.texture_image(object.icon) {
            if let Some(mut image) = image {
                if image.image != texture {
                    image.image = texture;
                }
            } else {
                commands.entity(entity).insert(ImageNode::new(texture));
            }
        }
    }
}
