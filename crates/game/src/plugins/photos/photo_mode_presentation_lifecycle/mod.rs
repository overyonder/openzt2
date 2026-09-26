use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::document::UiDocumentRole,
    world_definitions::immersive_mode_policy::ImmersiveModeKind,
};

use crate::plugins::{
    immersive_modes::immersive_mode_control_types::PhotoControl,
    ui::{
        animation::UiShowHideAnimation,
        authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot},
        ui_document_lifecycle_contracts::ShowUiRole,
    },
};

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use super::photo_capture_types::{Photo, PhotoCaptured};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AuthoredPhotoModeDocumentPresentationOwner {
    pub(super) controller: Entity,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ActiveLastCapturedPhotoPreview {
    photo: Entity,
    layout: Entity,
    image: Entity,
    elapsed_presented_frame_count: u16,
    maximum_presented_frame_count: u16,
}

pub(super) fn show_authored_photo_mode_document_for_entered_photo_mode(
    entered_photo_modes: Query<Entity, Added<PhotoControl>>,
    mut show_ui_roles: MessageWriter<ShowUiRole>,
    mut commands: Commands,
) {
    for controller_entity in &entered_photo_modes {
        let document_presentation_owner = commands
            .spawn((
                AuthoredPhotoModeDocumentPresentationOwner {
                    controller: controller_entity,
                },
                Visibility::Inherited,
            ))
            .id();
        show_ui_roles.write(ShowUiRole {
            role: UiDocumentRole::PhotoMode,
            owner: document_presentation_owner,
        });
    }
}

pub(super) fn initialize_authored_photo_mode_transient_surface_visibility_after_document_projection(
    document_presentation_owners: Query<(), With<AuthoredPhotoModeDocumentPresentationOwner>>,
    document_roots: Query<&ChildOf, With<UiDocumentRoot>>,
    mut projected_nodes: Query<
        (
            &Name,
            &UiDocumentOwner,
            &mut Visibility,
            &mut UiShowHideAnimation,
        ),
        Added<UiDocumentOwner>,
    >,
) {
    for (name, document_owner, mut visibility, mut animation) in &mut projected_nodes {
        let Ok(document_lifecycle_owner) = document_roots.get(document_owner.0) else {
            continue;
        };
        if !document_presentation_owners.contains(document_lifecycle_owner.parent()) {
            continue;
        }
        match name.as_str() {
            "cursor_mode_screen" => {
                *visibility = Visibility::Inherited;
                animation.start_authored_visibility_transition(true);
            }
            // The native Photo mode overrides this authored-visible node from
            // its current-challenge query. The current gameplay state has no
            // selected challenge, so project that absent state immediately.
            // The retained authored animation remains available when challenge
            // selection owns showing this surface.
            "photo challenges layout" => {
                *visibility = Visibility::Hidden;
                animation.start_authored_visibility_transition(false);
            }
            _ => {}
        }
    }
}

pub(super) fn hide_authored_photo_mode_document_after_photo_mode_exit(
    mut exited_photo_modes: RemovedComponents<PhotoControl>,
    document_presentation_owners: Query<(Entity, &AuthoredPhotoModeDocumentPresentationOwner)>,
    mut commands: Commands,
) {
    for controller_entity in exited_photo_modes.read() {
        document_presentation_owners
            .iter()
            .filter(|(_, owner)| owner.controller == controller_entity)
            .for_each(|(owner_entity, _)| commands.entity(owner_entity).despawn());
        commands
            .entity(controller_entity)
            .remove::<ActiveLastCapturedPhotoPreview>();
    }
}

/// Presents the canonical captured `Image` in the shipped Photo-mode preview.
/// The controller retains only UI/photo entity references and the authored
/// presentation counter; pixel and album ownership remain on the `Photo`.
pub(super) fn show_newly_captured_photo_in_authored_photo_mode_preview(
    mut captured_photos: MessageReader<PhotoCaptured>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    active_photo_mode_controllers: Query<Entity, With<PhotoControl>>,
    document_presentation_owners: Query<&AuthoredPhotoModeDocumentPresentationOwner>,
    document_roots: Query<(Entity, &UiDocumentRoot, &ChildOf)>,
    mut authored_nodes: Query<(
        Entity,
        &Name,
        &UiDocumentOwner,
        Option<&mut ImageNode>,
        &mut Visibility,
    )>,
    photos: Query<&Photo>,
    mut commands: Commands,
) {
    if captured_photos.is_empty() {
        return;
    }
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(maximum_presented_frame_count) = world_definitions
        .immersive_mode_policies()
        .find(|policy| policy.mode == ImmersiveModeKind::Photo)
        .map(|policy| policy.last_captured_photo_preview_frame_count)
    else {
        return;
    };
    let Ok(controller) = active_photo_mode_controllers.single() else {
        return;
    };
    let Some(document_root) = document_roots.iter().find_map(|(entity, root, parent)| {
        (document_presentation_owners
            .get(parent.parent())
            .is_ok_and(|owner| owner.controller == controller)
            && ui_document_assets
                .get(&root.document)
                .is_some_and(|document| {
                    document.canonical_ui_document().role == UiDocumentRole::PhotoMode
                }))
        .then_some(entity)
    }) else {
        return;
    };

    for captured in captured_photos.read() {
        let Ok(photo) = photos.get(captured.photo) else {
            continue;
        };
        let mut layout_entity = None;
        let mut image_entity = None;
        for (entity, name, owner, image_node, mut visibility) in &mut authored_nodes {
            if owner.0 != document_root {
                continue;
            }
            match name.as_str() {
                "Last Photo Layout" => {
                    *visibility = Visibility::Inherited;
                    layout_entity = Some(entity);
                }
                "Photo" => {
                    if let Some(mut image_node) = image_node {
                        image_node.image = photo.image.clone();
                        image_node.rect = None;
                        image_node.image_mode = NodeImageMode::Stretch;
                    } else {
                        let mut image_node = ImageNode::new(photo.image.clone());
                        image_node.image_mode = NodeImageMode::Stretch;
                        commands.entity(entity).insert(image_node);
                    }
                    image_entity = Some(entity);
                }
                _ => {}
            }
        }
        if let (Some(layout), Some(image)) = (layout_entity, image_entity) {
            commands
                .entity(controller)
                .insert(ActiveLastCapturedPhotoPreview {
                    photo: captured.photo,
                    layout,
                    image,
                    elapsed_presented_frame_count: 0,
                    maximum_presented_frame_count,
                });
        }
    }
}

pub(super) fn advance_and_retire_authored_last_captured_photo_preview(
    mut active_previews: Query<(Entity, &mut ActiveLastCapturedPhotoPreview), With<PhotoControl>>,
    photos: Query<&Photo>,
    mut preview_nodes: Query<(Option<&mut ImageNode>, &mut Visibility)>,
    mut commands: Commands,
) {
    for (controller, mut preview) in &mut active_previews {
        let Ok(photo) = photos.get(preview.photo) else {
            if let Ok((_, mut visibility)) = preview_nodes.get_mut(preview.layout) {
                *visibility = Visibility::Hidden;
            }
            commands
                .entity(controller)
                .remove::<ActiveLastCapturedPhotoPreview>();
            continue;
        };
        preview.elapsed_presented_frame_count =
            preview.elapsed_presented_frame_count.saturating_add(1);
        let expired = preview.elapsed_presented_frame_count > preview.maximum_presented_frame_count;
        if expired {
            if let Ok((_, mut visibility)) = preview_nodes.get_mut(preview.layout) {
                *visibility = Visibility::Hidden;
            }
            commands
                .entity(controller)
                .remove::<ActiveLastCapturedPhotoPreview>();
            continue;
        }
        if let Ok((Some(mut image_node), _)) = preview_nodes.get_mut(preview.image) {
            if image_node.image != photo.image {
                image_node.image = photo.image.clone();
            }
        }
        if let Ok((_, mut visibility)) = preview_nodes.get_mut(preview.layout) {
            *visibility = Visibility::Inherited;
        }
    }
}
