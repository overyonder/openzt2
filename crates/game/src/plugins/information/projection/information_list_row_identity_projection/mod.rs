use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::node_property_binding::{
        UiImagePropertyBindingSource, UiTextPropertyBindingSource,
    },
    AssetId,
};

use crate::assets::localization::localization_asset_types::LocalizationAsset;
use crate::assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use crate::assets::species::species_asset_types::SpeciesAsset;
use crate::assets::species::species_asset_types::SpeciesAssets;
use crate::assets::species::species_asset_types::SpeciesView;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_lifecycle::types::SpeciesHandle;
use crate::plugins::ui::authored_ui_image_content_binding::UiImageBinding;
use crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding;

use super::super::entity_selection_types::{InformationEntitySource, Inspectable};
use super::text_replacement_operations::replace_projected_ui_text_if_changed;

pub(in crate::plugins::information) fn project_world_subject_identity_to_information_list_rows(
    mut commands: Commands,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    species_assets: Res<Assets<SpeciesAsset>>,
    active_species: Res<SpeciesAssets>,
    information_entity_sources: Query<Ref<InformationEntitySource>>,
    changed_information_entity_sources: Query<(), Changed<InformationEntitySource>>,
    parent_relationships: Query<&ChildOf>,
    inspectable_world_subjects: Query<(&Inspectable, Option<&SpeciesHandle>, Option<Ref<Name>>)>,
    changed_subject_names: Query<(), (With<Inspectable>, Changed<Name>)>,
    mut text_nodes: Query<(Entity, &UiTextBinding, &mut Text)>,
    mut image_nodes: Query<(Entity, &UiImageBinding, Option<&mut ImageNode>)>,
) {
    let world_definitions_changed = world_definition_assets.is_changed();
    let localization_changed = localization_assets.is_changed() || active_localization.is_changed();
    let species_changed = species_assets.is_changed();
    let row_source_changed = !changed_information_entity_sources.is_empty();
    let subject_name_changed = !changed_subject_names.is_empty();
    if !world_definitions_changed
        && !localization_changed
        && !species_changed
        && !row_source_changed
        && !subject_name_changed
    {
        return;
    }
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(active_localization) =
        active_localization.borrow_loaded_localization_view(&localization_assets)
    else {
        return;
    };
    let Some(species_index) = active_species.get(&species_assets) else {
        return;
    };

    for (text_entity, text_binding, mut text) in &mut text_nodes {
        if !matches!(
            &text_binding.0,
            UiTextPropertyBindingSource::SelectedEntityName
                | UiTextPropertyBindingSource::SelectedEntityDefinitionName
        ) {
            continue;
        }
        let Some((world_subject, source_changed)) = find_information_list_row_source_in_ancestors(
            text_entity,
            &information_entity_sources,
            &parent_relationships,
        ) else {
            continue;
        };
        let Ok((inspectable, species_handle, subject_name)) =
            inspectable_world_subjects.get(world_subject)
        else {
            continue;
        };
        if !world_definitions_changed
            && !localization_changed
            && !species_changed
            && !source_changed
            && subject_name
                .as_ref()
                .is_none_or(|subject_name| !subject_name.is_changed())
        {
            continue;
        }
        let definition_id = resolve_world_definition_id_for_information_subject(
            inspectable,
            species_handle,
            species_index,
        );
        let Some(world_definition) = world_definitions.find_object(definition_id) else {
            continue;
        };
        if matches!(
            &text_binding.0,
            UiTextPropertyBindingSource::SelectedEntityName
        ) {
            if let Some(subject_name) = subject_name.as_ref() {
                replace_projected_ui_text_if_changed(&mut text.0, subject_name.as_str());
                continue;
            }
        }
        let definition_name_key = AssetId(world_definition.name_key.0);
        if active_localization
            .find_plain_localized_text(definition_name_key)
            .is_some()
        {
            text.0.clear();
            let _ = active_localization.write_localized_text_with_format_arguments(
                definition_name_key,
                &[],
                &mut text.0,
            );
        }
    }

    for (image_entity, image_binding, image_node) in &mut image_nodes {
        if !matches!(
            &image_binding.0,
            UiImagePropertyBindingSource::SelectedEntityIcon
        ) {
            continue;
        }
        let Some((world_subject, source_changed)) = find_information_list_row_source_in_ancestors(
            image_entity,
            &information_entity_sources,
            &parent_relationships,
        ) else {
            continue;
        };
        if !source_changed && !world_definitions_changed && !species_changed {
            continue;
        }
        let Ok((inspectable, species_handle, _)) = inspectable_world_subjects.get(world_subject)
        else {
            continue;
        };
        let definition_id = resolve_world_definition_id_for_information_subject(
            inspectable,
            species_handle,
            species_index,
        );
        let Some(icon_image) =
            world_definitions
                .find_object(definition_id)
                .and_then(|world_definition| {
                    world_definitions.texture_image(AssetId(world_definition.icon.0))
                })
        else {
            continue;
        };
        if let Some(mut image_node) = image_node {
            if image_node.image != icon_image {
                image_node.image = icon_image;
            }
        } else {
            commands
                .entity(image_entity)
                .insert(ImageNode::new(icon_image));
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

pub(super) fn find_information_list_row_source_in_ancestors(
    projected_row_descendant: Entity,
    information_entity_sources: &Query<Ref<InformationEntitySource>>,
    parent_relationships: &Query<&ChildOf>,
) -> Option<(Entity, bool)> {
    let mut ancestor_candidate = projected_row_descendant;
    for _ in 0..64 {
        if let Ok(information_entity_source) = information_entity_sources.get(ancestor_candidate) {
            return Some((
                information_entity_source.0,
                information_entity_source.is_changed(),
            ));
        }
        ancestor_candidate = parent_relationships.get(ancestor_candidate).ok()?.parent();
    }
    None
}
