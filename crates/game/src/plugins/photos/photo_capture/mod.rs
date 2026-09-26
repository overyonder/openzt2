//! Album-photo capture requests and conversion of completed screenshots into photo entities.

use bevy::{
    asset::RenderAssetUsages,
    camera::RenderTarget,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured as BevyScreenshotCaptured},
};

use crate::plugins::{
    immersive_modes::{
        immersive_mode_control_types::PhotoControl, immersive_mode_state_types::ActiveImmersiveMode,
    },
    simulation_time::simulation_clock_types::ZooClock,
    ui::authored_ui_node_projection_components::UiDocumentRoot,
    world_spawn::{
        persistent_id_types::PersistentIdAllocator, world_membership_types::WorldMember,
    },
};

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::ui_document::document::UiDocumentRole;

use super::{
    photo_album_types::{
        ActivePhotoAlbum, AlbumMember, CameraRollPhoto, PhotoAlbum, PhotoCameraRoll,
        PHOTO_CAMERA_ROLL_CAPACITY,
    },
    photo_capture_types::{
        CapturePhotoRequest, CapturedPhotoEvidence, CapturedPhotoSemantics, CapturedPhotoView,
        PendingPhotoCapture, PendingPhotoEvidence, PendingPhotoScore, PendingPhotoSemantics, Photo,
        PhotoCaptionSubject, PhotoCaptureFailed, PhotoCaptureOwner, PhotoFailure, PhotoMode,
        PhotoReadback, PhotoSubjects, PhotoSubjectsCollected, PhotoView,
    },
    photo_mode_presentation_lifecycle::AuthoredPhotoModeDocumentPresentationOwner,
};

#[derive(Component, Debug, Clone, Copy)]
pub(super) struct PhotoModePresentationHiddenDuringCapture {
    pub(super) root: Entity,
    pub(super) previous_visibility: Visibility,
}

pub(super) fn begin_requested_album_photo_captures(
    mut requests: MessageReader<CapturePhotoRequest>,
    clock: Res<ZooClock>,
    cameras: Query<
        (
            Entity,
            &Camera,
            &RenderTarget,
            &GlobalTransform,
            &PhotoMode,
            Option<&PhotoView>,
        ),
        Without<PendingPhotoCapture>,
    >,
    photo_cameras: Query<(), With<PhotoMode>>,
    camera_components: Query<(), With<Camera>>,
    active_photo_mode_controllers: Query<(Entity, &ActiveImmersiveMode), With<PhotoControl>>,
    photo_mode_document_presentation_owners: Query<&AuthoredPhotoModeDocumentPresentationOwner>,
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    mut ui_document_roots: Query<(Entity, &UiDocumentRoot, &ChildOf, &mut Visibility)>,
    mut readback: ResMut<PhotoReadback>,
    mut commands: Commands,
    mut failed: MessageWriter<PhotoCaptureFailed>,
) {
    for request in requests.read() {
        let result = if let Some(reason) = photo_capture_request_rejection(
            readback.busy,
            camera_components.contains(request.camera),
            photo_cameras.contains(request.camera),
            cameras.contains(request.camera),
        ) {
            Err(reason)
        } else {
            cameras
                .get(request.camera)
                .map_err(|_| PhotoFailure::InvalidCamera)
                .map(|(_, camera, target, transform, _, view)| {
                    (
                        target.clone(),
                        camera,
                        transform,
                        view.copied().unwrap_or_default(),
                    )
                })
        };
        let transform = match result {
            Ok((target, camera, transform, view)) if camera.is_active => (target, transform, view),
            Ok(_) => {
                failed.write(PhotoCaptureFailed {
                    reason: PhotoFailure::InvalidCamera,
                });
                continue;
            }
            Err(reason) => {
                failed.write(PhotoCaptureFailed { reason });
                continue;
            }
        };
        let (target, transform, view) = transform;
        let hidden_photo_mode_presentation = active_photo_mode_controllers
            .iter()
            .find(|(_, active_mode)| active_mode.camera == request.camera)
            .and_then(|(controller, _)| {
                ui_document_roots
                    .iter_mut()
                    .find(|(_, root, parent, _)| {
                        photo_mode_document_presentation_owners
                            .get(parent.parent())
                            .is_ok_and(|owner| owner.controller == controller)
                            && ui_document_assets
                                .get(&root.document)
                                .is_some_and(|document| {
                                    document.canonical_ui_document().role
                                        == UiDocumentRole::PhotoMode
                                })
                    })
                    .map(|(root, _, _, mut visibility)| {
                        let previous_visibility = *visibility;
                        *visibility = Visibility::Hidden;
                        PhotoModePresentationHiddenDuringCapture {
                            root,
                            previous_visibility,
                        }
                    })
            });
        let mut camera = commands.entity(request.camera);
        camera.insert((
            PendingPhotoCapture {
                captured_tick: clock.tick,
                camera_position: transform.translation(),
                camera_rotation: transform.rotation(),
                subjects: PhotoSubjects::default(),
                view,
            },
            PendingPhotoEvidence::default(),
            PendingPhotoSemantics::default(),
        ));
        if let Some(hidden_photo_mode_presentation) = hidden_photo_mode_presentation {
            camera.insert(hidden_photo_mode_presentation);
        }
        commands
            .spawn((Screenshot(target), PhotoCaptureOwner(request.camera)))
            .observe(create_album_photo_from_completed_screenshot_capture);
        readback.busy = true;
    }
}

fn create_album_photo_from_completed_screenshot_capture(
    captured: On<BevyScreenshotCaptured>,
    owners: Query<&PhotoCaptureOwner>,
    hidden_photo_mode_presentations: Query<&PhotoModePresentationHiddenDuringCapture>,
    mut readback: ResMut<PhotoReadback>,
    mut pending: Query<
        (
            Entity,
            &mut PendingPhotoCapture,
            &mut PendingPhotoEvidence,
            &mut PendingPhotoSemantics,
        ),
        With<PhotoSubjectsCollected>,
    >,
    albums: Query<
        (
            Entity,
            &WorldMember,
            Option<&ActivePhotoAlbum>,
            Option<&PhotoCameraRoll>,
        ),
        With<PhotoAlbum>,
    >,
    photos: Query<&AlbumMember, With<Photo>>,
    mut allocator: ResMut<PersistentIdAllocator>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut failed: MessageWriter<PhotoCaptureFailed>,
) {
    let Ok(owner) = owners.get(captured.entity) else {
        readback.busy = false;
        failed.write(PhotoCaptureFailed {
            reason: PhotoFailure::Gpu,
        });
        return;
    };
    let camera = owner.0;
    let hidden_photo_mode_presentation = hidden_photo_mode_presentations.get(camera).ok().copied();
    let Ok((_, mut capture, mut evidence, mut semantics)) = pending.get_mut(camera) else {
        restore_photo_mode_presentation_after_capture(
            &mut commands,
            hidden_photo_mode_presentation,
        );
        remove_pending_photo_capture_state(&mut commands, camera);
        readback.busy = false;
        failed.write(PhotoCaptureFailed {
            reason: PhotoFailure::Gpu,
        });
        return;
    };
    let album = albums
        .iter()
        .find(|row| row.3.is_some())
        .or_else(|| albums.iter().find(|row| row.2.is_some()))
        .or_else(|| albums.iter().next());
    let Some((album, member, _, camera_roll)) = album else {
        restore_photo_mode_presentation_after_capture(
            &mut commands,
            hidden_photo_mode_presentation,
        );
        remove_pending_photo_capture_state(&mut commands, camera);
        readback.busy = false;
        failed.write(PhotoCaptureFailed {
            reason: PhotoFailure::Storage,
        });
        return;
    };
    if camera_roll.is_some()
        && photos.iter().filter(|entry| entry.0 == album).count()
            >= PHOTO_CAMERA_ROLL_CAPACITY as usize
    {
        restore_photo_mode_presentation_after_capture(
            &mut commands,
            hidden_photo_mode_presentation,
        );
        remove_pending_photo_capture_state(&mut commands, camera);
        readback.busy = false;
        failed.write(PhotoCaptureFailed {
            reason: PhotoFailure::CapacityExceeded,
        });
        return;
    }
    let id = match allocator.allocate(member.root) {
        Ok(id) => id,
        Err(reason) => {
            restore_photo_mode_presentation_after_capture(
                &mut commands,
                hidden_photo_mode_presentation,
            );
            remove_pending_photo_capture_state(&mut commands, camera);
            readback.busy = false;
            failed.write(PhotoCaptureFailed {
                reason: PhotoFailure::PersistentId(reason),
            });
            return;
        }
    };
    let Ok(dynamic_image) = captured.image.clone().try_into_dynamic() else {
        restore_photo_mode_presentation_after_capture(
            &mut commands,
            hidden_photo_mode_presentation,
        );
        remove_pending_photo_capture_state(&mut commands, camera);
        readback.busy = false;
        failed.write(PhotoCaptureFailed {
            reason: PhotoFailure::Gpu,
        });
        return;
    };
    // Window targets are commonly BGRA. Normalize once at this explicit
    // capture boundary so both Bevy UI and persistent export consume one
    // tightly packed RGBA image without retaining target-format knowledge.
    let image = images.add(Image::from_dynamic(
        dynamic_image,
        true,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    ));
    let caption = evidence
        .0
        .iter()
        .max_by_key(|item| {
            u32::from(item.facts.screen_permille) + u32::from(item.facts.center_permille)
        })
        .map(|item| PhotoCaptionSubject(item.definition));
    let mut photo = commands.spawn((
        id,
        *member,
        Photo {
            image,
            captured_tick: capture.captured_tick,
            camera_position: capture.camera_position,
            camera_rotation: capture.camera_rotation,
            score_milli: 0,
        },
        PhotoSubjects(std::mem::take(&mut capture.subjects.0)),
        AlbumMember(album),
        CapturedPhotoEvidence(std::mem::take(&mut evidence.0)),
        CapturedPhotoSemantics(std::mem::take(&mut semantics.0)),
        CapturedPhotoView(capture.view),
        PendingPhotoScore,
    ));
    if let Some(caption) = caption {
        photo.insert(caption);
    }
    if camera_roll.is_some() {
        photo.insert(CameraRollPhoto);
    }
    restore_photo_mode_presentation_after_capture(&mut commands, hidden_photo_mode_presentation);
    remove_pending_photo_capture_state(&mut commands, camera);
    readback.busy = false;
}

fn remove_pending_photo_capture_state(commands: &mut Commands, camera: Entity) {
    commands
        .entity(camera)
        .remove::<PendingPhotoCapture>()
        .remove::<PendingPhotoEvidence>()
        .remove::<PendingPhotoSemantics>()
        .remove::<PhotoSubjectsCollected>()
        .remove::<PhotoModePresentationHiddenDuringCapture>();
}

pub(super) fn restore_photo_mode_presentation_after_capture(
    commands: &mut Commands,
    hidden_photo_mode_presentation: Option<PhotoModePresentationHiddenDuringCapture>,
) {
    if let Some(hidden) = hidden_photo_mode_presentation {
        commands
            .entity(hidden.root)
            .insert(hidden.previous_visibility);
    }
}

pub(crate) const fn photo_capture_request_rejection(
    readback_is_busy: bool,
    entity_has_camera: bool,
    entity_is_in_photo_mode: bool,
    camera_has_no_pending_capture: bool,
) -> Option<PhotoFailure> {
    if readback_is_busy || !camera_has_no_pending_capture {
        Some(PhotoFailure::ReadbackBusy)
    } else if !entity_has_camera {
        Some(PhotoFailure::InvalidCamera)
    } else if !entity_is_in_photo_mode {
        Some(PhotoFailure::NotInPhotoMode)
    } else {
        None
    }
}
