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
use openzt2_interface_state::immersive_modes::immersive_mode_state_types::ImmersiveCameraSessions;
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
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted,
    world_membership_types::{WorldMember, WorldRoot},
};

use super::{
    photo_album_types::{PHOTO_CAMERA_ROLL_CAPACITY, PhotoCameraRoll},
    photo_capture_types::{
        CapturedPhotoEvidence, PendingPhotoCapture, PendingPhotoEvidence, PendingPhotoScore,
        PhotoCaptureFailed, PhotoFailure, PhotoSubjectsCollected, PhotoView,
    },
    photo_mode_presentation_lifecycle::PhotoModePresentation,
};

#[derive(Component)]
#[relationship(relationship_target = PhotoViewfinderSuppressors)]
pub(super) struct SuppressesPhotoViewfinder(Entity);

#[derive(Component)]
#[relationship_target(relationship = SuppressesPhotoViewfinder)]
pub(super) struct PhotoViewfinderSuppressors(Vec<Entity>);

pub(super) fn begin_requested_album_photo_captures(
    mut requests: MessageReader<CapturePhotoRequest>,
    clock: Single<&ZooClock>,
    realtime: Res<Time<Real>>,
    cameras: Query<(&Camera, &RenderTarget, Has<PhotoMode>)>,
    pending_album_captures: Query<(), With<AlbumPhotoReadbackOf>>,
    screenshots: Query<&Screenshot, Without<Captured>>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    camera_sessions: Query<&ImmersiveCameraSessions>,
    presentations: Query<&PhotoModePresentation, With<PhotoControl>>,
    loaded_worlds: Query<Entity, (With<WorldRoot>, With<WorldLoadCompleted>)>,
    camera_rolls: Query<(Entity, &WorldMember), (With<PhotoCameraRoll>, With<PhotoAlbum>)>,
    selected_sets: Query<(&PhotoChallengeSet, &WorldMember), With<CurrentPhotoChallengeSet>>,
    mut commands: Commands,
    mut failed: MessageWriter<PhotoCaptureFailed>,
) {
    let clock = clock.into_inner();
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
        let destination = loaded_worlds.single().ok().and_then(|world| {
            camera_rolls
                .iter()
                .find(|(_, member)| member.root == world)
                .map(|(roll, _)| (world, roll))
        });
        let Some((world, camera_roll)) = destination else {
            failed.write(PhotoCaptureFailed {
                reason: PhotoFailure::Storage,
            });
            continue;
        };
        let challenge_set = selected_sets
            .iter()
            .find_map(|(set, member)| (member.root == world).then_some(set.id));
        let mut screenshot = commands.spawn((
            Screenshot(target.clone()),
            AlbumPhotoReadbackOf(request.camera),
            PendingPhotoCapture {
                world,
                camera_roll,
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
        let viewfinder = camera_sessions
            .get(request.camera)
            .ok()
            .and_then(|sessions| {
                sessions
                    .iter()
                    .find_map(|session| presentations.get(session).ok()?.iter().next())
            });
        if let Some(viewfinder) = viewfinder {
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
    camera_rolls: Query<
        (&WorldMember, Option<&AlbumPhotos>),
        (With<PhotoCameraRoll>, With<PhotoAlbum>),
    >,
    mut worlds: Query<&mut PersistentIdAllocator, (With<WorldRoot>, With<WorldLoadCompleted>)>,
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
        let (member, members) = camera_rolls
            .get(capture.camera_roll)
            .map_err(|_| PhotoFailure::Storage)?;
        if member.root != capture.world {
            return Err(PhotoFailure::Storage);
        }
        let mut allocator = worlds
            .get_mut(capture.world)
            .map_err(|_| PhotoFailure::Storage)?;
        if members.map_or(0, RelationshipTarget::len) >= PHOTO_CAMERA_ROLL_CAPACITY as usize {
            return Err(PhotoFailure::CapacityExceeded);
        }
        let id = allocator.allocate().map_err(PhotoFailure::PersistentId)?;
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
        commands.spawn((
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
            AlbumMember(capture.camera_roll),
            CameraRollPhoto,
            CapturedPhotoEvidence {
                subjects: std::mem::take(&mut evidence.0),
                challenge_set: capture.challenge_set,
                camera_position: capture.camera_position,
                view: capture.view,
            },
            PendingPhotoScore,
        ));
        Ok(())
    })();
    if let Err(reason) = result {
        failed.write(PhotoCaptureFailed { reason });
    }
}
