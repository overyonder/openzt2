//! Turn one presented frame into a persistent album exposure.

use std::time::Duration;

use bevy::{
    asset::RenderAssetUsages,
    camera::RenderTarget,
    ecs::lifecycle::Despawn,
    ecs::relationship::RelationshipTarget,
    prelude::*,
    render::view::screenshot::{
        Captured, Screenshot, ScreenshotCaptured as BevyScreenshotCaptured,
    },
    time::Real,
    window::PrimaryWindow,
};

use openzt2_interface_state::immersive_modes::immersive_mode_control_types::PhotoControl;
use openzt2_interface_state::immersive_modes::immersive_mode_state_types::ImmersiveCamera;
use openzt2_interface_state::photos::photo_album_types::{
    AlbumMember, AlbumPhotos, CameraRollPhoto, PhotoAlbum,
};
use openzt2_interface_state::photos::photo_capture_types::{
    AlbumPhotoReadbackOf, CapturePhotoRequest, Photo, PhotoMode, PhotoSubjects,
};
use openzt2_interface_state::photos::photo_challenge_types::{
    CurrentPhotoChallengeSet, PhotoChallengeSet,
};
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::{
    photo_album_types::{ActivePhotoAlbum, PhotoCameraRoll, PHOTO_CAMERA_ROLL_CAPACITY},
    photo_capture_types::{
        CapturedPhotoEvidence, PendingPhotoCapture, PendingPhotoEvidence, PendingPhotoScore,
        PhotoCaptionSubject, PhotoCaptureFailed, PhotoFailure, PhotoSubjectsCollected, PhotoView,
    },
    photo_mode_presentation_lifecycle::AuthoredPhotoModeDocumentPresentationOwner,
};

#[derive(Component)]
#[relationship(relationship_target = PhotoViewfinderSuppressors)]
pub(super) struct SuppressesPhotoViewfinder(Entity);

#[derive(Component)]
#[relationship_target(relationship = SuppressesPhotoViewfinder)]
pub(super) struct PhotoViewfinderSuppressors(Vec<Entity>);

pub(super) fn begin_requested_album_photo_captures(
    mut requests: MessageReader<CapturePhotoRequest>,
    clock: Res<ZooClock>,
    realtime: Res<Time<Real>>,
    cameras: Query<(&Camera, &RenderTarget, Has<PhotoMode>)>,
    pending_album_captures: Query<(), With<AlbumPhotoReadbackOf>>,
    screenshots: Query<&Screenshot, Without<Captured>>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    controllers: Query<&ImmersiveCamera, With<PhotoControl>>,
    viewfinders: Query<(Entity, &AuthoredPhotoModeDocumentPresentationOwner)>,
    allocator: Res<PersistentIdAllocator>,
    selected_sets: Query<(&PhotoChallengeSet, &WorldMember), With<CurrentPhotoChallengeSet>>,
    mut commands: Commands,
    mut failed: MessageWriter<PhotoCaptureFailed>,
) {
    let primary_window = primary_window.single().ok();
    let mut reserved = !pending_album_captures.is_empty();
    for request in requests.read() {
        let result = (|| {
            if reserved {
                return Err(PhotoFailure::ReadbackBusy);
            }
            let (camera, target, in_photo_mode) = cameras
                .get(request.camera)
                .map_err(|_| PhotoFailure::InvalidCamera)?;
            if !in_photo_mode {
                return Err(PhotoFailure::NotInPhotoMode);
            }
            let normalized = target
                .normalize(primary_window)
                .filter(|target| {
                    !matches!(target, bevy::camera::NormalizedRenderTarget::None { .. })
                })
                .ok_or(PhotoFailure::InvalidCamera)?;
            if !camera.is_active {
                return Err(PhotoFailure::InvalidCamera);
            }
            if screenshots.iter().any(|screenshot| {
                screenshot.0.normalize(primary_window).as_ref() == Some(&normalized)
            }) {
                return Err(PhotoFailure::ReadbackBusy);
            }
            Ok(target)
        })();
        let target = match result {
            Ok(capture) => capture,
            Err(reason) => {
                failed.write(PhotoCaptureFailed { reason });
                continue;
            }
        };
        let challenge_set = selected_sets
            .iter()
            .find_map(|(set, member)| allocator.owns_world(member.root).then_some(set.id));
        let mut screenshot = commands.spawn((
            Screenshot(target.clone()),
            AlbumPhotoReadbackOf(request.camera),
            PendingPhotoCapture {
                captured_tick: clock.tick,
                camera_position: Vec3::ZERO,
                camera_rotation: Quat::IDENTITY,
                view: PhotoView::default(),
                challenge_set,
                expires_at: realtime.elapsed() + Duration::from_secs(5),
                completed: false,
            },
            PendingPhotoEvidence::default(),
        ));
        screenshot
            .observe(create_album_photo_from_completed_screenshot_capture)
            .observe(reject_retired_album_capture);
        if let Some((viewfinder, _)) = viewfinders.iter().find(|(_, owner)| {
            controllers
                .get(owner.controller)
                .is_ok_and(|camera| camera.0 == request.camera)
        }) {
            screenshot.insert(SuppressesPhotoViewfinder(viewfinder));
        }
        reserved = true;
    }
}

pub(super) fn expire_pending_album_photo_captures(
    realtime: Res<Time<Real>>,
    cameras: Query<(), With<PhotoMode>>,
    pending: Query<(Entity, &AlbumPhotoReadbackOf, &PendingPhotoCapture)>,
    mut commands: Commands,
) {
    for (entity, camera, capture) in &pending {
        if !capture.completed
            && (realtime.elapsed() >= capture.expires_at || !cameras.contains(camera.0))
        {
            commands.entity(entity).despawn();
        }
    }
}

fn reject_retired_album_capture(
    retired: On<Despawn<PendingPhotoCapture>>,
    pending: Query<&PendingPhotoCapture>,
    mut failed: MessageWriter<PhotoCaptureFailed>,
) {
    if pending
        .get(retired.entity)
        .is_ok_and(|capture| !capture.completed)
    {
        failed.write(PhotoCaptureFailed {
            reason: PhotoFailure::Gpu,
        });
    }
}

fn create_album_photo_from_completed_screenshot_capture(
    captured: On<BevyScreenshotCaptured>,
    mut pending: Query<(
        &mut PendingPhotoCapture,
        &mut PendingPhotoEvidence,
        Has<PhotoSubjectsCollected>,
    )>,
    albums: Query<
        (
            Entity,
            &WorldMember,
            Has<ActivePhotoAlbum>,
            Has<PhotoCameraRoll>,
        ),
        With<PhotoAlbum>,
    >,
    members: Query<&AlbumPhotos>,
    mut allocator: ResMut<PersistentIdAllocator>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut failed: MessageWriter<PhotoCaptureFailed>,
) {
    let Ok((mut capture, mut evidence, collected)) = pending.get_mut(captured.entity) else {
        return;
    };
    if capture.completed {
        return;
    }
    capture.completed = true;
    commands
        .entity(captured.entity)
        .remove::<SuppressesPhotoViewfinder>();
    let result = (|| {
        if !collected {
            return Err(PhotoFailure::Gpu);
        }
        let world_albums = || albums.iter().filter(|row| allocator.owns_world(row.1.root));
        let (album, member, _, camera_roll) = world_albums()
            .find(|row| row.3)
            .or_else(|| world_albums().find(|row| row.2))
            .or_else(|| world_albums().next())
            .ok_or(PhotoFailure::Storage)?;
        if camera_roll
            && members.get(album).map_or(0, RelationshipTarget::len)
                >= PHOTO_CAMERA_ROLL_CAPACITY as usize
        {
            return Err(PhotoFailure::CapacityExceeded);
        }
        let id = allocator
            .allocate(member.root)
            .map_err(PhotoFailure::PersistentId)?;
        let pixels = captured
            .image
            .clone()
            .try_into_dynamic()
            .map_err(|_| PhotoFailure::Gpu)?;
        // Normalize target formats once for UI and persistent export.
        let image = images.add(Image::from_dynamic(
            pixels,
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
            PhotoSubjects(evidence.0.iter().map(|subject| subject.entity).collect()),
            AlbumMember(album),
            CapturedPhotoEvidence {
                subjects: std::mem::take(&mut evidence.0),
                challenge_set: capture.challenge_set,
                camera_position: capture.camera_position,
                view: capture.view,
            },
            PendingPhotoScore,
        ));
        if let Some(caption) = caption {
            photo.insert(caption);
        }
        if camera_roll {
            photo.insert(CameraRollPhoto);
        }
        Ok(())
    })();
    if let Err(reason) = result {
        failed.write(PhotoCaptureFailed { reason });
    }
}
