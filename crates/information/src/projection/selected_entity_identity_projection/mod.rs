use super::panel_binding_families::{IdentityIconFields, IdentityTextFieldOf, IdentityTextFields};
use bevy::{ecs::relationship::RelationshipTarget, prelude::*, text::EditableText};
use openzt2_game_data::{ui_document::node_property_binding::UiTextPropertyBindingSource, AssetId};

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;
use openzt2_interface_state::ui::authored_ui_visual_types::UiSourceRect;
use openzt2_interface_state::ui::authored_ui_visual_types::UiVisualLayer;

use super::text_replacement_operations::{
    replace_projected_editable_ui_text_if_changed, replace_projected_ui_text_if_changed,
};
use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_interface_state::information::entity_selection_types::{InfoPanel, SelectedSubject};

pub(crate) fn project_live_zoo_name_to_authored_editable_fields(
    zoos: Query<&Name, With<openzt2_world_state::world_spawn::world_membership_types::WorldRoot>>,
    mut edits: Query<(&UiTextBinding, &mut EditableText)>,
) {
    let Ok(name) = zoos.single() else { return };
    for (binding, edit) in &mut edits {
        if matches!(binding.0, UiTextPropertyBindingSource::ZooName) {
            replace_projected_editable_ui_text_if_changed(edit, name.as_str());
        }
    }
}

pub(crate) fn project_selected_entity_identity_to_authored_panel_members(
    mut commands: Commands,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    inspectable_world_subjects: Query<(&Inspectable, Option<&Name>)>,
    information_panels: Query<
        (
            &ChildOf,
            &InheritedVisibility,
            Option<&IdentityTextFields>,
            Option<&IdentityIconFields>,
        ),
        With<InfoPanel>,
    >,
    selections: Query<Ref<SelectedSubject>>,
    mut text_nodes: Query<(&UiTextBinding, &mut Text), With<IdentityTextFieldOf>>,
    children: Query<&Children>,
    mut visual_images: Query<
        (Entity, Option<&UiSourceRect>, Option<&mut ImageNode>),
        With<UiVisualLayer>,
    >,
) {
    let world_definitions = active_world_definitions.get(&world_definition_assets);
    let active_localization =
        active_localization.borrow_loaded_localization_view(&localization_assets);
    for (parent, inherited_visibility, text_fields, icon_fields) in &information_panels {
        let Ok(selection) = selections.get(parent.parent()) else {
            continue;
        };
        if !inherited_visibility.get() {
            continue;
        }
        let subject = inspectable_world_subjects.get(selection.0).ok();
        let world_definition =
            subject.and_then(|(subject, _)| world_definitions?.find_object(subject.definition));
        // A newly selected or reloading subject must never retain another
        // subject's identity while its definition/localization is unavailable.
        let localized_definition_name = world_definition
            .and_then(|definition| {
                active_localization?.find_plain_localized_text(AssetId(definition.name_key.0))
            })
            .unwrap_or_default();
        let live_name = subject
            .and_then(|(_, name)| name)
            .map_or(localized_definition_name, Name::as_str);
        let icon_image = world_definition
            .and_then(|definition| world_definitions?.texture_image(AssetId(definition.icon.0)));
        for entity in text_fields.into_iter().flat_map(RelationshipTarget::iter) {
            if let Ok((binding, text)) = text_nodes.get_mut(entity) {
                let name = match &binding.0 {
                    UiTextPropertyBindingSource::SelectedEntityName => Some(live_name),
                    UiTextPropertyBindingSource::SelectedEntityDefinitionName => {
                        Some(localized_definition_name)
                    }
                    _ => None,
                };
                if let Some(name) = name {
                    replace_projected_ui_text_if_changed(text, name);
                }
            }
        }
        for entity in icon_fields.into_iter().flat_map(RelationshipTarget::iter) {
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

pub(crate) fn project_selected_entity_name_to_authored_editable_fields(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    inspectable_world_subjects: Query<(&Inspectable, Option<&Name>)>,
    information_panels: Query<
        (&ChildOf, &InheritedVisibility, &IdentityTextFields),
        With<InfoPanel>,
    >,
    selections: Query<Ref<SelectedSubject>>,
    mut editable_name_nodes: Query<(&UiTextBinding, &mut EditableText), With<IdentityTextFieldOf>>,
) {
    let world_definitions = active_world_definitions.get(&world_definition_assets);
    let active_localization =
        active_localization.borrow_loaded_localization_view(&localization_assets);
    for (parent, inherited_visibility, text_fields) in &information_panels {
        let Ok(selection) = selections.get(parent.parent()) else {
            continue;
        };
        if !inherited_visibility.get() {
            continue;
        }
        let selected_entity_name = inspectable_world_subjects
            .get(selection.0)
            .ok()
            .and_then(|(subject, live_name)| {
                live_name.map(Name::as_str).or_else(|| {
                    let definition = world_definitions?.find_object(subject.definition)?;
                    active_localization?.find_plain_localized_text(AssetId(definition.name_key.0))
                })
            })
            .unwrap_or_default();
        for entity in text_fields.iter() {
            let Ok((text_binding, editable_text)) = editable_name_nodes.get_mut(entity) else {
                continue;
            };
            if matches!(
                &text_binding.0,
                UiTextPropertyBindingSource::SelectedEntityName
            ) {
                replace_projected_editable_ui_text_if_changed(editable_text, selected_entity_name);
            }
        }
    }
}
