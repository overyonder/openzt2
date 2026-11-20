//! Plain camera screenshots for export.

use bevy::{
    camera::{NormalizedRenderTarget, RenderTarget},
    prelude::*,
    render::view::screenshot::{
        Captured, Screenshot, ScreenshotCaptured as BevyScreenshotCaptured,
    },
    window::PrimaryWindow,
};

use openzt2_interface_state::photos::photo_capture_types::{
    CaptureScreenshotRequest, ScreenshotCaptured,
};

use super::photo_capture_types::{ScreenshotCaptureFailed, ScreenshotCaptureOwner};

pub(super) fn begin_requested_camera_screenshot_captures(
    mut requests: MessageReader<CaptureScreenshotRequest>,
    cameras: Query<(&Camera, &RenderTarget)>,
    screenshots: Query<&Screenshot, Without<Captured>>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    mut commands: Commands,
    mut failed: MessageWriter<ScreenshotCaptureFailed>,
) {
    if requests.is_empty() {
        return;
    }
    let primary_window = primary_window.single().ok();
    let mut targets = screenshots
        .iter()
        .filter_map(|screenshot| screenshot.0.normalize(primary_window))
        .collect::<std::collections::BTreeSet<_>>();
    for request in requests.read() {
        let target = cameras
            .get(request.camera)
            .ok()
            .filter(|(camera, _)| camera.is_active)
            .map(|(_, target)| target);
        let Some(target) = target.filter(|target| {
            target
                .normalize(primary_window)
                .filter(|target| !matches!(target, NormalizedRenderTarget::None { .. }))
                .is_some_and(|target| targets.insert(target))
        }) else {
            failed.write(ScreenshotCaptureFailed {
                camera: request.camera,
            });
            continue;
        };
        commands
            .spawn((
                Screenshot(target.clone()),
                ScreenshotCaptureOwner(request.camera),
            ))
            .observe(publish_completed_camera_screenshot_image);
    }
}

fn publish_completed_camera_screenshot_image(
    captured: On<BevyScreenshotCaptured>,
    owners: Query<&ScreenshotCaptureOwner>,
    mut images: ResMut<Assets<Image>>,
    mut completed: MessageWriter<ScreenshotCaptured>,
) {
    let Ok(owner) = owners.get(captured.entity) else {
        return;
    };
    completed.write(ScreenshotCaptured {
        camera: owner.0,
        image: images.add(captured.image.clone()),
    });
}
