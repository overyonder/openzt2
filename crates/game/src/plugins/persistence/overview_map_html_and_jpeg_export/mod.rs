//! Explicit asynchronous export of the live rendered overview-map presentation.

use std::{io, path::PathBuf};

use arrayvec::ArrayVec;
use bevy::{
    camera::RenderTarget,
    ecs::system::SystemParam,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured},
    tasks::{block_on, poll_once, IoTaskPool, Task},
};
use image::{codecs::jpeg::JpegEncoder, ExtendedColorType};

use crate::plugins::{
    camera::camera_runtime_state_types::ZooCamera,
    information::overview::overview_types::ExportOverviewMap,
    ui::authored_ui_node_projection_components::{UiDocumentOwner, UiNodeId},
};

use super::{
    durable_filesystem_operations::atomically_write_and_sync_file,
    persistence_filesystem_paths::PersistenceFilesystemPaths, profile_types::ProfileOptions,
};

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OverviewMapHtmlAndJpegExported;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OverviewMapHtmlAndJpegExportFailed;

#[derive(Component)]
struct OverviewMapRenderedPresentationCapture {
    directory: PathBuf,
    hidden_controls: ArrayVec<(Entity, Visibility), 2>,
}

#[derive(Component)]
pub(super) struct OverviewMapHtmlAndJpegWriteTask(Task<io::Result<()>>);

#[derive(SystemParam)]
pub(super) struct OverviewMapRenderedPresentationCaptureParameters<'w, 's> {
    directories: Option<Res<'w, PersistenceFilesystemPaths>>,
    profile: Res<'w, ProfileOptions>,
    zoo_cameras: Query<'w, 's, (&'static Camera, &'static RenderTarget), With<ZooCamera>>,
    ui_nodes: Query<
        'w,
        's,
        (
            Entity,
            &'static UiDocumentOwner,
            &'static UiNodeId,
            &'static mut Visibility,
        ),
    >,
    pending_capture: Query<'w, 's, (), With<OverviewMapRenderedPresentationCapture>>,
    commands: Commands<'w, 's>,
    failed: MessageWriter<'w, OverviewMapHtmlAndJpegExportFailed>,
}

pub(super) fn begin_overview_map_html_and_jpeg_exports(
    mut requests: MessageReader<ExportOverviewMap>,
    mut capture: OverviewMapRenderedPresentationCaptureParameters,
) {
    let mut capture_started = !capture.pending_capture.is_empty();
    for request in requests.read() {
        let Some(directories) = capture.directories.as_deref() else {
            capture.failed.write(OverviewMapHtmlAndJpegExportFailed);
            continue;
        };
        let Some(render_target) = capture
            .zoo_cameras
            .iter()
            .find_map(|(camera, target)| camera.is_active.then_some(target.clone()))
        else {
            capture.failed.write(OverviewMapHtmlAndJpegExportFailed);
            continue;
        };
        if capture_started {
            capture.failed.write(OverviewMapHtmlAndJpegExportFailed);
            continue;
        }

        let mut hidden_controls = ArrayVec::new();
        for (entity, owner, node_id, mut visibility) in &mut capture.ui_nodes {
            if owner.0 == request.document_owner
                && (entity == request.export_control || node_id.id == request.return_control)
                && hidden_controls.try_push((entity, *visibility)).is_ok()
            {
                *visibility = Visibility::Hidden;
            }
        }
        if hidden_controls.len() != 2 {
            for (entity, previous_visibility) in &hidden_controls {
                if let Ok((_, _, _, mut visibility)) = capture.ui_nodes.get_mut(*entity) {
                    *visibility = *previous_visibility;
                }
            }
            capture.failed.write(OverviewMapHtmlAndJpegExportFailed);
            continue;
        }

        let directory =
            directories.overview_map_export_directory_path(&capture.profile.profile_identifier.0);
        capture
            .commands
            .spawn((
                Screenshot(render_target),
                OverviewMapRenderedPresentationCapture {
                    directory,
                    hidden_controls,
                },
            ))
            .observe(begin_writing_captured_overview_map_presentation);
        capture_started = true;
    }
}

fn begin_writing_captured_overview_map_presentation(
    captured: On<ScreenshotCaptured>,
    export_requests: Query<&OverviewMapRenderedPresentationCapture>,
    mut control_visibilities: Query<&mut Visibility>,
    mut commands: Commands,
    mut failed: MessageWriter<OverviewMapHtmlAndJpegExportFailed>,
) {
    let Ok(capture) = export_requests.get(captured.entity) else {
        failed.write(OverviewMapHtmlAndJpegExportFailed);
        return;
    };
    restore_overview_map_export_control_visibilities(
        &capture.hidden_controls,
        &mut control_visibilities,
    );

    let directory = capture.directory.clone();
    let image = captured.image.clone();
    commands.spawn(OverviewMapHtmlAndJpegWriteTask(
        IoTaskPool::get().spawn(async move {
            let rgb = image
                .try_into_dynamic()
                .map_err(io::Error::other)?
                .into_rgb8();
            let width = rgb.width();
            let height = rgb.height();
            let mut jpeg = Vec::new();
            JpegEncoder::new_with_quality(&mut jpeg, 92)
                .encode(rgb.as_raw(), width, height, ExtendedColorType::Rgb8)
                .map_err(io::Error::other)?;
            atomically_write_and_sync_file(&directory.join("overviewmap.jpg"), &jpeg)?;
            let html = format!(
                "<!doctype html><html><head><meta charset=\"utf-8\"><title>Overview Map</title></head><body><img src=\"overviewmap.jpg\" width=\"{width}\" height=\"{height}\" alt=\"Overview Map\"></body></html>\n"
            );
            atomically_write_and_sync_file(
                &directory.join("overviewmap.html"),
                html.as_bytes(),
            )
        }),
    ));
}

fn restore_overview_map_export_control_visibilities(
    hidden_controls: &[(Entity, Visibility)],
    control_visibilities: &mut Query<&mut Visibility>,
) {
    for (entity, visibility) in hidden_controls {
        if let Ok(mut current_visibility) = control_visibilities.get_mut(*entity) {
            *current_visibility = *visibility;
        }
    }
}

pub(super) fn complete_overview_map_html_and_jpeg_exports(
    mut tasks: Query<(Entity, &mut OverviewMapHtmlAndJpegWriteTask)>,
    mut commands: Commands,
    mut completed: MessageWriter<OverviewMapHtmlAndJpegExported>,
    mut failed: MessageWriter<OverviewMapHtmlAndJpegExportFailed>,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = block_on(poll_once(&mut task.0)) else {
            continue;
        };
        if result.is_ok() {
            completed.write(OverviewMapHtmlAndJpegExported);
        } else {
            failed.write(OverviewMapHtmlAndJpegExportFailed);
        }
        commands.entity(entity).despawn();
    }
}
