use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::node_property_binding::{
        UiImagePropertyBindingSource, UiTextPropertyBindingSource,
    },
    AssetId,
};

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::species::species_asset_types::SpeciesAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAssets;
use openzt2_game_assets::species::species_asset_types::SpeciesView;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_ui::authored_ui_image_content_binding::UiImageBinding;
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;

use super::{information_row_field_binding::InformationRowFieldOf, InformationRows};
use bevy::ecs::relationship::RelationshipTarget;
use openzt2_interface_state::information::entity_selection_types::Inspectable;

pub(crate) fn project_world_subject_identity_to_information_list_rows(
    mut commands: Commands,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    species_assets: Res<Assets<SpeciesAsset>>,
    active_species: Res<SpeciesAssets>,
    sources: InformationRows,
    subjects: Query<(
        Ref<Inspectable>,
        Option<Ref<SpeciesHandle>>,
        Option<Ref<Name>>,
    )>,
    mut text_nodes: Query<(&UiTextBinding, &mut Text)>,
    mut image_nodes: Query<(&UiImageBinding, Option<&mut ImageNode>)>,
    mut formatted: Local<String>,
    new_bindings: Query<
        &InformationRowFieldOf,
        Or<(Changed<UiTextBinding>, Changed<UiImageBinding>)>,
    >,
    mut removed: (RemovedComponents<Name>, RemovedComponents<SpeciesHandle>),
) {
    if sources.is_empty() {
        return;
    }
    let refresh_rows = active_world_definitions.is_changed()
        || world_definition_assets.is_changed()
        || active_localization.is_changed()
        || localization_assets.is_changed()
        || species_assets.is_changed()
        || active_species.is_changed();
    let removed_subject_facts = removed
        .0
        .read()
        .chain(removed.1.read())
        .collect::<bevy::ecs::entity::EntityHashSet>();
    let Some(definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(localization) =
        active_localization.borrow_loaded_localization_view(&localization_assets)
    else {
        return;
    };
    let Some(species) = active_species.get(&species_assets) else {
        return;
    };
    for (source, bindings) in &sources {
        let Ok((inspectable, species_handle, name)) = subjects.get(source.0) else {
            continue;
        };
        if !refresh_rows
            && !bindings.is_changed()
            && !bindings.iter().any(|field| new_bindings.contains(field))
            && !removed_subject_facts.contains(&source.0)
            && !source.is_changed()
            && !inspectable.is_changed()
            && !species_handle.as_ref().is_some_and(Ref::is_changed)
            && !name.as_ref().is_some_and(Ref::is_changed)
        {
            continue;
        }
        let id = resolve_world_definition_id_for_information_subject(
            &inspectable,
            species_handle.as_deref(),
            species,
        );
        let Some(definition) = definitions.find_object(id) else {
            continue;
        };
        let mut formatted_name = false;
        for entity in bindings.iter() {
            if let Ok((binding, mut text)) = text_nodes.get_mut(entity) {
                let next = match &binding.0 {
                    UiTextPropertyBindingSource::SelectedEntityName if name.is_some() => {
                        name.as_deref().map(Name::as_str)
                    }
                    UiTextPropertyBindingSource::SelectedEntityName
                    | UiTextPropertyBindingSource::SelectedEntityDefinitionName => {
                        let key = AssetId(definition.name_key.0);
                        if localization.find_plain_localized_text(key).is_some() {
                            if !formatted_name {
                                formatted.clear();
                                let _ = localization.write_localized_text_with_format_arguments(
                                    key,
                                    &[],
                                    &mut *formatted,
                                );
                                formatted_name = true;
                            }
                            Some(formatted.as_str())
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some(next) = next.filter(|next| text.0 != *next) {
                    text.0.clear();
                    text.0.push_str(next);
                }
            }
            if let Ok((binding, image)) = image_nodes.get_mut(entity) {
                if !matches!(binding.0, UiImagePropertyBindingSource::SelectedEntityIcon) {
                    continue;
                }
                let Some(icon) = definitions.texture_image(AssetId(definition.icon.0)) else {
                    continue;
                };
                if let Some(mut image) = image {
                    if image.image != icon {
                        image.image = icon;
                    }
                } else {
                    commands.entity(entity).insert(ImageNode::new(icon));
                }
            }
        }
    }
}

fn resolve_world_definition_id_for_information_subject(
    inspectable: &Inspectable,
    species_handle: Option<&SpeciesHandle>,
    species_index: SpeciesView<'_>,
) -> AssetId {
    species_handle
        .and_then(|species_handle| species_index.find(species_handle.species))
        .map(|species_record| AssetId(species_record.world_definition.0))
        .unwrap_or(inspectable.definition)
}
