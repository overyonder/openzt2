use openzt2_interface_state::photos::photo_capture_types::Photo;

use bevy::{
    ecs::{lifecycle::Remove, relationship::RelationshipTarget},
    prelude::*,
};
use openzt2_game_data::{
    ui_document::document::UiDocumentRole,
    world_definitions::immersive_mode_policy::ImmersiveModeKind,
};

use openzt2_interface_state::immersive_modes::immersive_mode_control_types::PhotoControl;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentMembers;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::ui_document_lifecycle_contracts::ShowUiRole;
use openzt2_ui::animation::UiShowHideAnimation;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use super::photo_capture::PhotoViewfinderSuppressors;
use super::photo_capture_types::PhotoCaptured;

#[derive(Component)]
pub(super) struct PhotoChallengeGoals;

#[derive(Component)]
pub(super) struct PhotoChallengeGoalList;

#[derive(Component)]
#[relationship(relationship_target = PhotoModePresentation)]
pub(super) struct PhotoModePresentationOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = PhotoModePresentationOf, linked_spawn)]
pub(super) struct PhotoModePresentation(Entity);

#[derive(Component, PartialEq, Eq)]
pub(super) struct ActiveLastCapturedPhotoPreview {
    photo: Entity,
    layout: Entity,
    remaining_presented_frames: u16,
}

pub(super) fn show_authored_photo_mode_document_for_entered_photo_mode(
    entered_photo_modes: Query<Entity, (Added<PhotoControl>, Without<PhotoModePresentation>)>,
    mut show_ui_roles: MessageWriter<ShowUiRole>,
    mut commands: Commands,
) {
    for controller in &entered_photo_modes {
        let owner = commands
            .spawn((PhotoModePresentationOf(controller), Visibility::Inherited))
            .id();
        show_ui_roles.write(ShowUiRole {
            role: UiDocumentRole::PhotoMode,
            owner,
        });
    }
}

pub(super) fn retire_photo_mode_presentation_when_control_is_removed(
    removed: On<Remove<PhotoControl>>,
    presentations: Query<&PhotoModePresentation>,
    mut commands: Commands,
) {
    if let Ok(presentation) = presentations.get(removed.entity) {
        if let Some(owner) = presentation.iter().next() {
            commands.entity(owner).try_despawn();
        }
    }
}

pub(super) fn initialize_authored_photo_mode_transient_surface_visibility_after_document_projection(
    mut commands: Commands,
    document_presentation_owners: Query<(), With<PhotoModePresentationOf>>,
    document_roots: Query<&ChildOf, With<UiDocumentRoot>>,
    mut projected_nodes: Query<
        (
            Entity,
            &Name,
            &UiDocumentOwner,
            &mut Visibility,
            Option<&mut UiShowHideAnimation>,
        ),
        Added<UiDocumentOwner>,
    >,
) {
    for (entity, name, document_owner, mut visibility, animation) in &mut projected_nodes {
        let Ok(document_lifecycle_owner) = document_roots.get(document_owner.0) else {
            continue;
        };
        if !document_presentation_owners.contains(document_lifecycle_owner.parent()) {
            continue;
        }
        match name.as_str() {
            "cursor_mode_screen" => {
                visibility.set_if_neq(Visibility::Inherited);
                if let Some(mut animation) = animation {
                    animation.start_authored_visibility_transition(true);
                }
            }
            "photo challenges layout" => {
                commands.entity(entity).insert(PhotoChallengeGoals);
            }
            "photo challenges listbox" => {
                commands.entity(entity).insert(PhotoChallengeGoalList);
            }
            _ => {}
        }
    }
}

pub(super) fn project_photo_viewfinder_capture_visibility(
    mut viewfinders: Query<
        &mut Visibility,
        (
            With<PhotoModePresentationOf>,
            Changed<PhotoViewfinderSuppressors>,
        ),
    >,
) {
    for mut visibility in &mut viewfinders {
        visibility.set_if_neq(Visibility::Hidden);
    }
}

pub(super) fn restore_photo_viewfinder_visibility_after_capture(
    removed: On<Remove<PhotoViewfinderSuppressors>>,
    mut viewfinders: Query<&mut Visibility, With<PhotoModePresentationOf>>,
) {
    if let Ok(mut visibility) = viewfinders.get_mut(removed.entity) {
        visibility.set_if_neq(Visibility::Inherited);
    }
}

pub(super) fn show_newly_captured_photo_in_authored_photo_mode_preview(
    mut captured_photos: MessageReader<PhotoCaptured>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    active_photo_mode_controllers: Query<&PhotoModePresentation, With<PhotoControl>>,
    mut presentations: Query<(&Children, Option<&mut ActiveLastCapturedPhotoPreview>)>,
    document_roots: Query<(&UiDocumentRoot, &UiDocumentMembers)>,
    mut authored_nodes: Query<(&Name, Option<&mut ImageNode>, &mut Visibility)>,
    photos: Query<&Photo>,
    mut commands: Commands,
) {
    if captured_photos.is_empty() {
        return;
    }
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(frame_count) = world_definitions
        .immersive_mode_policies()
        .find(|policy| policy.mode == ImmersiveModeKind::Photo)
        .map(|policy| policy.last_captured_photo_preview_frame_count)
    else {
        return;
    };
    let Some(owner) = active_photo_mode_controllers
        .single()
        .ok()
        .and_then(|presentation| presentation.iter().next())
    else {
        return;
    };
    let Ok((children, existing_preview)) = presentations.get_mut(owner) else {
        return;
    };
    let Some(members) = RelationshipTarget::iter(children).find_map(|entity| {
        let (root, members) = document_roots.get(entity).ok()?;
        ui_document_assets
            .get(&root.document)
            .is_some_and(|document| {
                document.canonical_ui_document().role == UiDocumentRole::PhotoMode
            })
            .then_some(members)
    }) else {
        return;
    };
    let Some((photo_entity, photo)) = captured_photos
        .read()
        .filter_map(|capture| Some((capture.photo, photos.get(capture.photo).ok()?)))
        .last()
    else {
        return;
    };
    if frame_count == 0 {
        if existing_preview.is_some() {
            commands
                .entity(owner)
                .try_remove::<ActiveLastCapturedPhotoPreview>();
        }
        return;
    }
    let mut layout = None;
    let mut image = None;
    for entity in members.iter() {
        let Ok((name, ..)) = authored_nodes.get(entity) else {
            continue;
        };
        match name.as_str() {
            "Last Photo Layout" => layout = Some(entity),
            "Photo" => image = Some(entity),
            _ => {}
        }
    }
    let (Some(layout), Some(image)) = (layout, image) else {
        return;
    };
    if let Ok((_, image_node, _)) = authored_nodes.get_mut(image) {
        if let Some(mut image_node) = image_node {
            image_node.image = photo.image.clone();
            image_node.rect = None;
            image_node.image_mode = NodeImageMode::Stretch;
        } else {
            let mut image_node = ImageNode::new(photo.image.clone());
            image_node.image_mode = NodeImageMode::Stretch;
            commands.entity(image).insert(image_node);
        }
    }
    if let Ok((_, _, mut visibility)) = authored_nodes.get_mut(layout) {
        visibility.set_if_neq(Visibility::Inherited);
    }
    let preview = ActiveLastCapturedPhotoPreview {
        photo: photo_entity,
        layout,
        remaining_presented_frames: frame_count - 1,
    };
    if let Some(mut current) = existing_preview {
        if current.layout != layout {
            if let Ok((_, _, mut visibility)) = authored_nodes.get_mut(current.layout) {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
        current.set_if_neq(preview);
    } else {
        commands.entity(owner).insert(preview);
    }
}

pub(super) fn advance_and_retire_authored_last_captured_photo_preview(
    mut active_previews: Query<(Entity, &mut ActiveLastCapturedPhotoPreview)>,
    photos: Query<(), With<Photo>>,
    mut commands: Commands,
) {
    for (owner, mut preview) in &mut active_previews {
        if preview.remaining_presented_frames == 0 || !photos.contains(preview.photo) {
            commands
                .entity(owner)
                .try_remove::<ActiveLastCapturedPhotoPreview>();
        } else {
            preview.remaining_presented_frames -= 1;
        }
    }
}

pub(super) fn hide_retired_photo_preview_layout(
    removed: On<Remove<ActiveLastCapturedPhotoPreview>>,
    previews: Query<&ActiveLastCapturedPhotoPreview>,
    mut visibility: Query<&mut Visibility>,
) {
    if let Ok(preview) = previews.get(removed.entity) {
        if let Ok(mut layout) = visibility.get_mut(preview.layout) {
            layout.set_if_neq(Visibility::Hidden);
        }
    }
}
