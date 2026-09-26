//! Routing of authored photography actions to direct album operations.
use openzt2_game_data::ui_document::action::UiTrigger;

use openzt2_game_data::ui_document::action::photography::UiPhotoAction;

use bevy::prelude::*;

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::{
        immersive_modes::{
            immersive_mode_control_types::PhotoControl,
            immersive_mode_message_types::{ExitImmersiveMode, ModeExitReason},
            immersive_mode_state_types::ActiveImmersiveMode,
        },
        ui::{
            authored_ui_node_projection_components::UiDocumentOwner,
            authored_ui_node_projection_components::UiDocumentRoot,
        },
        world_spawn::{
            persistent_id_types::PersistentIdAllocator, world_membership_types::WorldMember,
        },
    },
};

use super::album_operations::{
    change_active_photo_album_page_by_signed_offset, deselect_photo_in_album_display_slot,
    find_photo_camera_roll_album, find_photo_in_album_display_slot,
    reorder_photos_for_insertion_at_album_position, request_deletion_of_selected_photo_from_album,
    request_deletion_of_selected_photo_from_any_album, select_photo_album_by_list_index,
    select_photo_in_album_display_slot,
};
use super::photo_album_types::{
    ActivePhotoAlbum, AlbumMember, CameraRollPhoto, DeletePhotoRequest, EnlargedAlbumPhoto,
    ExportPhotoAlbum, PhotoAlbum, PhotoAlbumCapacityPages, PhotoAlbumChoiceRow, PhotoAlbumOrder,
    PhotoAlbumPage, PhotoCameraRoll, PhotoCameraRollRow, PhotoHelpShown, PhotoMove,
    PhotoMoveTarget, PhotoPointerDrag, PresentedPhotoCount, SelectedAlbumPhoto,
    INITIAL_PHOTO_ALBUM_SPREADS, PHOTO_ALBUM_PAGE_SIZE,
};
use super::photo_capture_types::{CapturePhotoRequest, Photo};
use crate::plugins::ui::authored_ui_action_projection_components::UiPhotoActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

/// Applies selection emitted by the shipped reusable exposure and album-entry
/// rows to their canonical album entities. The row components contain only
/// entity references; photo images, names, membership, and active selection
/// remain owned by the photo/album entities themselves.
pub(super) fn select_photo_storage_entity_from_authored_row_activation(
    mut activations: MessageReader<UiNodeActivated>,
    parents: Query<&ChildOf>,
    camera_roll_rows: Query<&PhotoCameraRollRow>,
    album_choice_rows: Query<&PhotoAlbumChoiceRow>,
    albums: Query<
        (
            Entity,
            Option<&SelectedAlbumPhoto>,
            Has<ActivePhotoAlbum>,
            Has<PhotoCameraRoll>,
        ),
        With<PhotoAlbum>,
    >,
    mut commands: Commands,
) {
    for activation in activations.read() {
        let mut row = std::iter::successors(Some(activation.node), |entity| {
            parents.get(*entity).ok().map(ChildOf::parent)
        });
        if let Some(photo) = row
            .clone()
            .find_map(|entity| camera_roll_rows.get(entity).ok())
        {
            let Some(camera_roll) = albums.iter().find(|row| row.3).map(|row| row.0) else {
                continue;
            };
            match activation.trigger {
                UiTrigger::On => {
                    for (album, ..) in &albums {
                        commands.entity(album).remove::<SelectedAlbumPhoto>();
                    }
                    commands
                        .entity(camera_roll)
                        .insert(SelectedAlbumPhoto(photo.0));
                }
                UiTrigger::Off => {
                    if albums
                        .get(camera_roll)
                        .ok()
                        .and_then(|row| row.1)
                        .is_some_and(|selected| selected.0 == photo.0)
                    {
                        commands.entity(camera_roll).remove::<SelectedAlbumPhoto>();
                    }
                }
                _ => {}
            }
            continue;
        }
        if activation.trigger != UiTrigger::On {
            continue;
        }
        if let Some(choice) = row.find_map(|entity| album_choice_rows.get(entity).ok()) {
            for (album, ..) in &albums {
                commands.entity(album).remove::<ActivePhotoAlbum>();
            }
            commands.entity(choice.0).insert(ActivePhotoAlbum);
        }
    }
}

pub(super) fn route_photo_ui_actions(
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    action_nodes: Query<(&UiPhotoActions, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    albums: Query<(
        Entity,
        &PhotoAlbum,
        Option<&WorldMember>,
        Option<&PhotoAlbumPage>,
        Option<&SelectedAlbumPhoto>,
        Option<&PhotoMove>,
        Option<&PhotoCameraRoll>,
    )>,
    names: Query<&Name, With<PhotoAlbum>>,
    active_albums: Query<Entity, (With<PhotoAlbum>, With<ActivePhotoAlbum>)>,
    capacities: Query<&PhotoAlbumCapacityPages, With<PhotoAlbum>>,
    photos: Query<(Entity, &Photo, &AlbumMember, Option<&PhotoAlbumOrder>)>,
    mut allocator: ResMut<PersistentIdAllocator>,
    mut deletions: MessageWriter<DeletePhotoRequest>,
    mut captures: MessageWriter<CapturePhotoRequest>,
    mut immersive_mode_exits: MessageWriter<ExitImmersiveMode>,
    mut exports: MessageWriter<ExportPhotoAlbum>,
    active_photo_modes: Query<(Entity, &ActiveImmersiveMode), With<PhotoControl>>,
    mut commands: Commands,
) {
    for activation in activations.read() {
        let Ok((range, owner)) = action_nodes.get(activation.node) else {
            continue;
        };
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let records = range.authored_action_records(document);
        for record in records {
            if activation.trigger != record.trigger {
                continue;
            }
            let active = active_albums
                .iter()
                .next()
                .or_else(|| albums.iter().next().map(|row| row.0));
            match &record.action {
                UiPhotoAction::CapturePhotoFromActivePhotoModeCamera => {
                    if let Ok((_, active_photo_mode)) = active_photo_modes.single() {
                        captures.write(CapturePhotoRequest {
                            camera: active_photo_mode.camera,
                        });
                    }
                }
                UiPhotoAction::ExitActivePhotoMode => {
                    if let Ok((controller, _)) = active_photo_modes.single() {
                        immersive_mode_exits.write(ExitImmersiveMode {
                            controller,
                            reason: ModeExitReason::Cancelled,
                        });
                    }
                }
                UiPhotoAction::RefreshPresentedActivePhotoAlbumPhotoCount => {
                    if let Some(album) = active {
                        let count = photos
                            .iter()
                            .filter(|(_, _, member, _)| member.0 == album)
                            .count() as u32;
                        commands.entity(album).insert(PresentedPhotoCount(count));
                    }
                }
                UiPhotoAction::RefreshActivePhotoAlbumPage => {
                    if let Some(album) = active {
                        commands.entity(album).insert(PhotoAlbumPage(
                            albums
                                .get(album)
                                .ok()
                                .and_then(|row| row.3)
                                .map_or(0, |page| page.0),
                        ));
                    }
                }
                UiPhotoAction::EnlargeSelectedPhotoInActiveAlbum => {
                    if let Some(album) = active {
                        if let Ok((_, _, _, _, Some(selected), ..)) = albums.get(album) {
                            commands
                                .entity(album)
                                .insert(EnlargedAlbumPhoto(selected.0));
                        }
                    }
                }
                UiPhotoAction::StopEnlargingPhotoInActiveAlbum => {
                    if let Some(album) = active {
                        commands.entity(album).remove::<EnlargedAlbumPhoto>();
                    }
                }
                UiPhotoAction::ShowPreviousActivePhotoAlbumPage => {
                    change_active_photo_album_page_by_signed_offset(
                        active,
                        -1,
                        &albums,
                        &capacities,
                        &photos,
                        &mut commands,
                    )
                }
                UiPhotoAction::ShowNextActivePhotoAlbumPage => {
                    change_active_photo_album_page_by_signed_offset(
                        active,
                        1,
                        &albums,
                        &capacities,
                        &photos,
                        &mut commands,
                    )
                }
                UiPhotoAction::CreateNewPhotoAlbumFromActiveAlbumProfile => {
                    if let Some((album_entity, source, member, ..)) =
                        active.and_then(|id| albums.get(id).ok())
                    {
                        if let (Some(member), Ok(name)) = (member, names.get(album_entity)) {
                            if let Ok(id) = allocator.allocate(member.root) {
                                let album = commands
                                    .spawn((
                                        PhotoAlbum {
                                            profile: source.profile,
                                        },
                                        Name::new(name.as_str().to_owned()),
                                        *member,
                                        id,
                                        ActivePhotoAlbum,
                                        PhotoAlbumPage::default(),
                                    ))
                                    .id();
                                if let Some(previous) = active {
                                    commands.entity(previous).remove::<ActivePhotoAlbum>();
                                }
                                commands
                                    .entity(album)
                                    .insert(PhotoAlbumCapacityPages(INITIAL_PHOTO_ALBUM_SPREADS));
                            }
                        }
                    }
                }
                UiPhotoAction::MarkPhotoAlbumHelpAsShown => {
                    if let Some(album) = active {
                        commands.entity(album).insert(PhotoHelpShown);
                    }
                }
                UiPhotoAction::DeleteActivePhotoAlbumAndContainedPhotos => {
                    if let Some(album) = active {
                        photos
                            .iter()
                            .filter(|(_, _, member, _)| member.0 == album)
                            .for_each(|(photo, _, _, _)| {
                                deletions.write(DeletePhotoRequest { photo });
                            });
                        commands.entity(album).despawn();
                        if let Some((candidate, ..)) =
                            albums.iter().find(|(candidate, ..)| *candidate != album)
                        {
                            commands.entity(candidate).insert(ActivePhotoAlbum);
                        }
                    }
                }
                UiPhotoAction::StartMovingSelectedPhotoToActiveAlbum => {
                    if let Some(album) = active {
                        if let Some((from, selected)) = albums
                            .iter()
                            .find_map(|row| row.4.map(|selected| (row.0, selected.0)))
                        {
                            commands.entity(album).insert(PhotoMove {
                                photo: selected,
                                from,
                            });
                        }
                    }
                }
                UiPhotoAction::CancelMovingSelectedPhoto => {
                    if let Some(album) = active {
                        commands
                            .entity(album)
                            .remove::<PhotoMove>()
                            .remove::<PhotoPointerDrag>()
                            .remove::<PhotoMoveTarget>();
                    }
                }
                UiPhotoAction::ExportActivePhotoAlbumAsHtml => {
                    if let Some(album) = active {
                        exports.write(ExportPhotoAlbum { album });
                    }
                }
                UiPhotoAction::SelectPhotoInActiveAlbumSlot {
                    album_slot_index: index,
                } => select_photo_in_album_display_slot(
                    active,
                    *index,
                    &albums,
                    &photos,
                    &mut commands,
                ),
                UiPhotoAction::DeselectPhotoInActiveAlbumSlot {
                    album_slot_index: index,
                } => deselect_photo_in_album_display_slot(
                    active,
                    *index,
                    &albums,
                    &photos,
                    &mut commands,
                ),
                UiPhotoAction::EnlargePhotoInActiveAlbumSlot {
                    album_slot_index: index,
                } => {
                    if let Some(photo) =
                        find_photo_in_album_display_slot(active, *index, &albums, &photos)
                    {
                        if let Some(album) = active {
                            commands.entity(album).insert(EnlargedAlbumPhoto(photo));
                        }
                    }
                }
                UiPhotoAction::SelectPhotoInCameraRollSlot {
                    camera_roll_slot_index: index,
                } => select_photo_in_album_display_slot(
                    find_photo_camera_roll_album(&albums),
                    *index,
                    &albums,
                    &photos,
                    &mut commands,
                ),
                UiPhotoAction::DeselectPhotoInCameraRollSlot {
                    camera_roll_slot_index: index,
                } => deselect_photo_in_album_display_slot(
                    find_photo_camera_roll_album(&albums),
                    *index,
                    &albums,
                    &photos,
                    &mut commands,
                ),
                UiPhotoAction::SelectPhotoAlbumByListIndex {
                    album_list_index: index,
                } => select_photo_album_by_list_index(*index, &albums, &mut commands),
                UiPhotoAction::AddOnePageToActivePhotoAlbumCapacity => {
                    if let Some(album) = active {
                        let pages = capacities
                            .get(album)
                            .map_or(INITIAL_PHOTO_ALBUM_SPREADS, |capacity| {
                                capacity.0.saturating_add(1)
                            });
                        commands
                            .entity(album)
                            .insert(PhotoAlbumCapacityPages(pages));
                    }
                }
                UiPhotoAction::SetHoveredPhotoMoveTargetSlot {
                    album_slot_index: index,
                } => {
                    if let Some(album) = active {
                        if let Ok(slot) = u8::try_from(*index) {
                            commands.entity(album).insert(PhotoMoveTarget(slot));
                        }
                    }
                }
                UiPhotoAction::ClearHoveredPhotoMoveTargetSlot {
                    album_slot_index: index,
                } => {
                    if let Some(album) = active {
                        if albums.get(album).ok().and_then(|row| row.5).is_some()
                            && u8::try_from(*index).is_ok()
                        {
                            commands.entity(album).remove::<PhotoMoveTarget>();
                        }
                    }
                }
                UiPhotoAction::MoveSelectedPhotoToActiveAlbumSlot {
                    album_slot_index: index,
                } => {
                    if let Some(album) = active {
                        if let Ok((_, _, _, _, _, Some(moving), _)) = albums.get(album) {
                            let target = albums
                                .get(album)
                                .ok()
                                .and_then(|row| row.3)
                                .map_or(0, |page| page.0)
                                * PHOTO_ALBUM_PAGE_SIZE
                                + u32::try_from(*index).unwrap_or_default();
                            reorder_photos_for_insertion_at_album_position(
                                album,
                                moving.photo,
                                target,
                                &photos,
                                &mut commands,
                            );
                            commands
                                .entity(moving.photo)
                                .insert((AlbumMember(album), PhotoAlbumOrder(target as u64)))
                                .remove::<CameraRollPhoto>();
                            if moving.from != album {
                                commands.entity(moving.from).remove::<SelectedAlbumPhoto>();
                            }
                            commands
                                .entity(album)
                                .remove::<PhotoMove>()
                                .remove::<PhotoPointerDrag>()
                                .remove::<PhotoMoveTarget>()
                                .insert(SelectedAlbumPhoto(moving.photo));
                        }
                    }
                }
                UiPhotoAction::DeleteAllPhotosFromCameraRoll => {
                    if let Some(roll) = find_photo_camera_roll_album(&albums) {
                        photos
                            .iter()
                            .filter(|(_, _, member, _)| member.0 == roll)
                            .for_each(|(photo, _, _, _)| {
                                deletions.write(DeletePhotoRequest { photo });
                            });
                    }
                }
                UiPhotoAction::DeleteSelectedPhotoFromAnyAlbum => {
                    request_deletion_of_selected_photo_from_any_album(&albums, &mut deletions)
                }
                UiPhotoAction::DeleteSelectedPhotoFromActiveAlbum => {
                    request_deletion_of_selected_photo_from_album(active, &albums, &mut deletions)
                }
                UiPhotoAction::DeleteSelectedPhotoFromCameraRoll => {
                    request_deletion_of_selected_photo_from_album(
                        find_photo_camera_roll_album(&albums),
                        &albums,
                        &mut deletions,
                    )
                }
            }
        }
    }
}
