//! Plain camera screenshots which do not enter photo-album semantics.

use bevy::{
    camera::RenderTarget,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured as BevyScreenshotCaptured},
};

use super::photo_capture_types::{
    CaptureScreenshotRequest, ScreenshotCaptureFailed, ScreenshotCaptureOwner, ScreenshotCaptured,
};

/// Starts a plain screenshot readback without entering album/photo semantics.
///
/// Bevy owns target extraction and GPU readback. The photo system retains only the request
/// owner on Bevy's short-lived screenshot entity and publishes the resulting
/// canonical `Image` handle once.
pub(super) fn begin_requested_camera_screenshot_captures(
    mut requests: MessageReader<CaptureScreenshotRequest>,
    cameras: Query<(&Camera, &RenderTarget)>,
    mut commands: Commands,
    mut failed: MessageWriter<ScreenshotCaptureFailed>,
) {
    for request in requests.read() {
        let Ok((camera, target)) = cameras.get(request.camera) else {
            failed.write(ScreenshotCaptureFailed {
                camera: request.camera,
            });
            continue;
        };
        if !camera.is_active {
            failed.write(ScreenshotCaptureFailed {
                camera: request.camera,
            });
            continue;
        }
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
