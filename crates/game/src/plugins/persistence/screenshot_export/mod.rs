//! Explicit asynchronous export of Bevy-owned full-frame screenshots.

use std::{
    io,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use bevy::{
    prelude::*,
    tasks::{block_on, poll_once, IoTaskPool, Task},
};

use crate::plugins::photos::photo_capture_types::ScreenshotCaptured;

use super::{
    durable_filesystem_operations::atomically_write_and_sync_file,
    persistence_filesystem_paths::PersistenceFilesystemPaths,
    rgba8_bmp_encoding::encode_tightly_packed_rgba8_as_bgra_bmp,
};

#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub(super) struct FullFrameScreenshotBmpExported {
    pub camera: Entity,
    pub path: PathBuf,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FullFrameScreenshotBmpExportFailed {
    pub camera: Entity,
}

#[derive(Component)]
pub(super) struct FullFrameScreenshotBmpWriteTask {
    camera: Entity,
    path: PathBuf,
    task: Task<io::Result<()>>,
}

pub(super) fn begin_full_frame_screenshot_bmp_exports(
    mut captured: MessageReader<ScreenshotCaptured>,
    directories: Option<Res<PersistenceFilesystemPaths>>,
    images: Res<Assets<Image>>,
    mut commands: Commands,
    mut failed: MessageWriter<FullFrameScreenshotBmpExportFailed>,
) {
    for captured in captured.read() {
        let Some(directories) = directories.as_deref() else {
            failed.write(FullFrameScreenshotBmpExportFailed {
                camera: captured.camera,
            });
            continue;
        };
        let Some(image) = images.get(&captured.image) else {
            failed.write(FullFrameScreenshotBmpExportFailed {
                camera: captured.camera,
            });
            continue;
        };
        let Ok(dynamic) = image.clone().try_into_dynamic() else {
            failed.write(FullFrameScreenshotBmpExportFailed {
                camera: captured.camera,
            });
            continue;
        };
        let rgba = dynamic.into_rgba8();
        let bytes = match encode_tightly_packed_rgba8_as_bgra_bmp(
            rgba.width(),
            rgba.height(),
            rgba.as_raw(),
        ) {
            Ok(bytes) => bytes,
            Err(_) => {
                failed.write(FullFrameScreenshotBmpExportFailed {
                    camera: captured.camera,
                });
                continue;
            }
        };
        let captured_unix_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let path = directories.timestamped_full_frame_screenshot_bmp_file_path(captured_unix_ns);
        let task_path = path.clone();
        commands.spawn(FullFrameScreenshotBmpWriteTask {
            camera: captured.camera,
            path,
            task: IoTaskPool::get()
                .spawn(async move { atomically_write_and_sync_file(&task_path, bytes.as_slice()) }),
        });
    }
}

pub(super) fn complete_full_frame_screenshot_bmp_exports(
    mut tasks: Query<(Entity, &mut FullFrameScreenshotBmpWriteTask)>,
    mut commands: Commands,
    mut completed: MessageWriter<FullFrameScreenshotBmpExported>,
    mut failed: MessageWriter<FullFrameScreenshotBmpExportFailed>,
) {
    for (entity, mut export) in &mut tasks {
        let Some(result) = block_on(poll_once(&mut export.task)) else {
            continue;
        };
        if result.is_ok() {
            completed.write(FullFrameScreenshotBmpExported {
                camera: export.camera,
                path: export.path.clone(),
            });
        } else {
            failed.write(FullFrameScreenshotBmpExportFailed {
                camera: export.camera,
            });
        }
        commands.entity(entity).despawn();
    }
}
