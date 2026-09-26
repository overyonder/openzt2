use bevy::{prelude::*, text::EditableText};
use openzt2_game_data::{
    ui_document::node_property_binding::{
        UiImagePropertyBindingSource, UiTextPropertyBindingSource,
    },
    AssetId,
};

use crate::assets::localization::localization_asset_types::LocalizationAsset;
use crate::assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::ui::authored_ui_image_content_binding::UiImageBinding;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding;
use crate::plugins::ui::authored_ui_visual_types::UiSourceRect;
use crate::plugins::ui::authored_ui_visual_types::UiVisualLayer;

use super::super::entity_selection_types::{InfoPanel, Inspectable};
use super::text_replacement_operations::{
    replace_projected_editable_ui_text_if_changed, replace_projected_ui_text_if_changed,
};

pub(in crate::plugins::information) fn project_live_zoo_name_to_authored_editable_fields(
    zoos: Query<&Name, With<crate::plugins::world_spawn::world_membership_types::WorldRoot>>,
    mut edits: Query<(&UiTextBinding, &mut EditableText)>,
) {
    let Ok(name) = zoos.single() else { return };
    for (binding, mut edit) in &mut edits {
        if matches!(binding.0, UiTextPropertyBindingSource::ZooName) {
            replace_projected_editable_ui_text_if_changed(&mut edit, name.as_str());
        }
    }
}

pub(in crate::plugins::information) fn project_authored_definition_name_to_selected_entity_panel(
    mut commands: Commands,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    inspectable_world_subjects: Query<&Inspectable>,
    information_panels: Query<(Entity, &InfoPanel, &InheritedVisibility)>,
    mut text_nodes: Query<(&UiDocumentOwner, &UiTextBinding, &mut Text)>,
    image_nodes: Query<(Entity, &UiDocumentOwner, &UiImageBinding)>,
    children: Query<&Children>,
    mut visual_images: Query<
        (Entity, Option<&UiSourceRect>, Option<&mut ImageNode>),
        With<UiVisualLayer>,
    >,
) {
    let world_definitions = active_world_definitions.get(&world_definition_assets);
    let active_localization =
        active_localization.borrow_loaded_localization_view(&localization_assets);
    for (panel_entity, information_panel, inherited_visibility) in &information_panels {
        if !inherited_visibility.get() {
            continue;
        }
        let world_definition = inspectable_world_subjects
            .get(information_panel.subject)
            .ok()
            .and_then(|subject| world_definitions?.find_object(subject.definition));
        // A newly selected or reloading subject must never retain another
        // subject's identity while its definition/localization is unavailable.
        let localized_definition_name = world_definition
            .and_then(|definition| {
                active_localization?.find_plain_localized_text(AssetId(definition.name_key.0))
            })
            .unwrap_or_default();
        for (document_owner, text_binding, mut text) in &mut text_nodes {
            if document_owner.0 == panel_entity
                && matches!(
                    &text_binding.0,
                    UiTextPropertyBindingSource::SelectedEntityName
                        | UiTextPropertyBindingSource::SelectedEntityDefinitionName
                )
            {
                replace_projected_ui_text_if_changed(&mut text.0, localized_definition_name);
            }
        }
        let icon_image = world_definition
            .and_then(|definition| world_definitions?.texture_image(AssetId(definition.icon.0)));
        for (entity, document_owner, image_binding) in &image_nodes {
            if document_owner.0 != panel_entity
                || !matches!(
                    &image_binding.0,
                    UiImagePropertyBindingSource::SelectedEntityIcon
                )
            {
                continue;
            }
            let Ok(children) = children.get(entity) else {
                continue;
            };
            for child in children.iter() {
                let Ok((visual_entity, source_rect, image_node)) = visual_images.get_mut(child)
                else {
                    continue;
                };
                let Some(icon_image) = icon_image.as_ref() else {
                    if image_node.is_some() {
                        commands.entity(visual_entity).remove::<ImageNode>();
                    }
                    continue;
                };
                let source_rect = source_rect.map(|source_rect| Rect {
                    min: Vec2::new(source_rect.0[0] as f32, source_rect.0[1] as f32),
                    max: Vec2::new(
                        (source_rect.0[0] + source_rect.0[2]) as f32,
                        (source_rect.0[1] + source_rect.0[3]) as f32,
                    ),
                });
                if let Some(mut image_node) = image_node {
                    if image_node.image != *icon_image {
                        image_node.image = icon_image.clone();
                    }
                    if image_node.image_mode != NodeImageMode::Stretch {
                        image_node.image_mode = NodeImageMode::Stretch;
                    }
                } else {
                    let mut image_node = ImageNode::new(icon_image.clone());
                    image_node.rect = source_rect;
                    image_node.image_mode = NodeImageMode::Stretch;
                    commands.entity(visual_entity).insert(image_node);
                }
            }
        }
    }
}

/// Uses the live name when present, otherwise the definition name.
pub(in crate::plugins::information) fn project_live_name_to_selected_entity_panel(
    information_panels: Query<(Entity, &InfoPanel, &InheritedVisibility)>,
    inspectable_world_subjects: Query<(&Inspectable, Option<&Name>)>,
    mut text_nodes: Query<(&UiDocumentOwner, &UiTextBinding, &mut Text)>,
) {
    for (panel_entity, information_panel, inherited_visibility) in &information_panels {
        if !inherited_visibility.get() {
            continue;
        }
        let Ok((_, Some(subject_name))) = inspectable_world_subjects.get(information_panel.subject)
        else {
            continue;
        };
        for (document_owner, text_binding, mut text) in &mut text_nodes {
            if document_owner.0 == panel_entity
                && matches!(
                    &text_binding.0,
                    UiTextPropertyBindingSource::SelectedEntityName
                )
            {
                replace_projected_ui_text_if_changed(&mut text.0, subject_name.as_str());
            }
        }
    }
}

pub(in crate::plugins::information) fn project_selected_entity_name_to_authored_editable_fields(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    inspectable_world_subjects: Query<(&Inspectable, Option<&Name>)>,
    information_panels: Query<(Entity, &InfoPanel, &InheritedVisibility)>,
    mut editable_name_nodes: Query<(&UiDocumentOwner, &UiTextBinding, &mut EditableText)>,
) {
    let world_definitions = active_world_definitions.get(&world_definition_assets);
    let active_localization =
        active_localization.borrow_loaded_localization_view(&localization_assets);
    for (panel_entity, information_panel, inherited_visibility) in &information_panels {
        if !inherited_visibility.get() {
            continue;
        }
        let selected_entity_name = inspectable_world_subjects
            .get(information_panel.subject)
            .ok()
            .and_then(|(subject, live_name)| {
                live_name.map(Name::as_str).or_else(|| {
                    let definition = world_definitions?.find_object(subject.definition)?;
                    active_localization?.find_plain_localized_text(AssetId(definition.name_key.0))
                })
            })
            .unwrap_or_default();
        for (document_owner, text_binding, mut editable_text) in &mut editable_name_nodes {
            if document_owner.0 == panel_entity
                && matches!(
                    &text_binding.0,
                    UiTextPropertyBindingSource::SelectedEntityName
                )
            {
                replace_projected_editable_ui_text_if_changed(
                    &mut editable_text,
                    selected_entity_name,
                );
            }
        }
    }
}
