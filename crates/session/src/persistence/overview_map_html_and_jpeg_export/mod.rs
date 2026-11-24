use std::{io, path::PathBuf};

use bevy::{
    camera::{visibility::RenderLayers, ImageRenderTarget, RenderTarget},
    ecs::query::QueryData,
    prelude::*,
    render::{
        render_resource::{TextureFormat, TextureUsages},
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
    tasks::{block_on, poll_once, IoTaskPool, Task},
    text::{LetterSpacing, LineHeight},
};
use image::{codecs::jpeg::JpegEncoder, ExtendedColorType};

use openzt2_game_data::AssetId;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_interface_state::information::overview::overview_types::ExportOverviewMap;
use openzt2_interface_state::persistence::profile_types::ProfileOptions;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiNodeId;

use super::{
    durable_filesystem_operations::atomically_write_and_sync_file,
    html_export_result_opening::HtmlExportWritten,
    persistence_filesystem_paths::PersistenceFilesystemPaths,
};

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OverviewMapHtmlAndJpegExportFailed;

#[derive(Component)]
pub(super) struct OverviewExportProjection {
    directory: PathBuf,
    root: Entity,
}

#[derive(Component)]
pub(super) struct OverviewExportReadbackStarted;

#[derive(Component)]
pub(super) struct OverviewMapHtmlAndJpegWriteTask(Task<io::Result<PathBuf>>);

#[derive(QueryData)]
pub(super) struct OverviewPresentationNode {
    children: Option<&'static Children>,
    identity: Option<&'static UiNodeId>,
    node: Option<&'static Node>,
    visibility: Option<&'static Visibility>,
    transform: Option<&'static UiTransform>,
    image: Option<&'static ImageNode>,
    background: Option<&'static BackgroundColor>,
    border: Option<&'static BorderColor>,
    outline: Option<&'static Outline>,
    z_index: Option<&'static ZIndex>,
    global_z_index: Option<&'static GlobalZIndex>,
    scroll: Option<&'static ScrollPosition>,
    text: Option<&'static Text>,
    span: Option<&'static TextSpan>,
    font: Option<&'static TextFont>,
    text_color: Option<&'static TextColor>,
    text_layout: Option<&'static TextLayout>,
    text_shadow: Option<&'static TextShadow>,
    line_height: Option<&'static LineHeight>,
    letter_spacing: Option<&'static LetterSpacing>,
}

pub(super) fn begin_overview_map_html_and_jpeg_exports(
    mut commands: Commands,
    mut requests: MessageReader<ExportOverviewMap>,
    directories: Option<Res<PersistenceFilesystemPaths>>,
    profile: Res<ProfileOptions>,
    zoo_cameras: Query<&Camera, With<ZooCamera>>,
    nodes: Query<OverviewPresentationNode>,
    pending: Query<(), With<OverviewExportProjection>>,
    mut images: ResMut<Assets<Image>>,
    mut failed: MessageWriter<OverviewMapHtmlAndJpegExportFailed>,
) {
    let mut capture_started = !pending.is_empty();
    let output = zoo_cameras
        .iter()
        .find(|camera| camera.is_active)
        .and_then(|camera| {
            Some((
                camera.physical_target_size()?,
                camera.target_scaling_factor()?,
            ))
        });
    for request in requests.read() {
        let (Some(directories), Some((size, scale))) = (directories.as_deref(), output) else {
            failed.write(OverviewMapHtmlAndJpegExportFailed);
            continue;
        };
        if capture_started || size.x == 0 || size.y == 0 || !nodes.contains(request.document_owner)
        {
            failed.write(OverviewMapHtmlAndJpegExportFailed);
            continue;
        }
        let mut image =
            Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None);
        image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let target = images.add(image);
        let projection = commands
            .spawn((
                Camera2d,
                Camera {
                    order: -1,
                    ..Default::default()
                },
                RenderTarget::Image(ImageRenderTarget {
                    handle: target,
                    scale_factor: scale,
                }),
                RenderLayers::none(),
            ))
            .id();
        let omitted = [
            request.return_control,
            AssetId::from_key("maintooltip"),
            AssetId::from_key("floatingtooltip"),
        ];
        if let Some(root) = spawn_overview_export_node(
            &mut commands,
            &nodes,
            request.document_owner,
            None,
            projection,
            request.export_control,
            &omitted,
            false,
        ) {
            commands
                .entity(projection)
                .insert(OverviewExportProjection {
                    directory: directories
                        .overview_map_export_directory_path(&profile.profile_identifier.0),
                    root,
                });
            capture_started = true;
        } else {
            commands.entity(projection).despawn();
            failed.write(OverviewMapHtmlAndJpegExportFailed);
        }
    }
}

fn spawn_overview_export_node(
    commands: &mut Commands,
    nodes: &Query<OverviewPresentationNode>,
    source: Entity,
    parent: Option<Entity>,
    camera: Entity,
    export_control: Entity,
    omitted: &[AssetId],
    hidden_ancestor: bool,
) -> Option<Entity> {
    let node = nodes.get(source).ok()?;
    let mut clone = commands.spawn_empty();
    if let Some(parent) = parent {
        clone.insert(ChildOf(parent));
    } else {
        clone.insert(UiTargetCamera(camera));
    }
    if let Some(layout) = node.node {
        clone.insert((layout.clone(), Pickable::IGNORE));
    }
    let hidden = hidden_ancestor
        || source == export_control
        || node.identity.is_some_and(|id| omitted.contains(&id.id));
    if hidden {
        clone.insert(Visibility::Hidden);
    } else if let Some(visibility) = node.visibility {
        clone.insert(*visibility);
    }
    if let Some(value) = node.transform {
        clone.insert(*value);
    }
    if let Some(value) = node.image {
        clone.insert(value.clone());
    }
    if let Some(value) = node.background {
        clone.insert(*value);
    }
    if let Some(value) = node.border {
        clone.insert(value.clone());
    }
    if let Some(value) = node.outline {
        clone.insert(value.clone());
    }
    if let Some(value) = node.z_index {
        clone.insert(*value);
    }
    if let Some(value) = node.global_z_index {
        clone.insert(*value);
    }
    if let Some(value) = node.scroll {
        clone.insert(value.clone());
    }
    if let Some(value) = node.text {
        clone.insert(value.clone());
    }
    if let Some(value) = node.span {
        clone.insert(value.clone());
    }
    if let Some(value) = node.font {
        clone.insert(value.clone());
    }
    if let Some(value) = node.text_color {
        clone.insert(*value);
    }
    if let Some(value) = node.text_layout {
        clone.insert(*value);
    }
    if let Some(value) = node.text_shadow {
        clone.insert(*value);
    }
    if let Some(value) = node.line_height {
        clone.insert(*value);
    }
    if let Some(value) = node.letter_spacing {
        clone.insert(*value);
    }
    let parent = clone.id();
    if let Some(children) = node.children {
        for &child in children {
            let _ = spawn_overview_export_node(
                commands,
                nodes,
                child,
                Some(parent),
                camera,
                export_control,
                omitted,
                hidden,
            );
        }
    }
    Some(parent)
}

pub(super) fn capture_overview_export_projection(
    mut commands: Commands,
    projections: Query<
        (Entity, &Camera, &RenderTarget),
        (
            With<OverviewExportProjection>,
            Without<OverviewExportReadbackStarted>,
        ),
    >,
) {
    for (entity, camera, target) in &projections {
        if camera.physical_target_size().is_none() {
            continue;
        }
        commands
            .entity(entity)
            .insert(OverviewExportReadbackStarted);
        commands
            .spawn((Screenshot(target.clone()), ChildOf(entity)))
            .observe(begin_writing_captured_overview_map_presentation);
    }
}

fn begin_writing_captured_overview_map_presentation(
    captured: On<ScreenshotCaptured>,
    screenshots: Query<&ChildOf>,
    projections: Query<&OverviewExportProjection>,
    mut commands: Commands,
    mut failed: MessageWriter<OverviewMapHtmlAndJpegExportFailed>,
) {
    let Ok(projection) = screenshots.get(captured.entity) else {
        failed.write(OverviewMapHtmlAndJpegExportFailed);
        return;
    };
    let Ok(export) = projections.get(projection.parent()) else {
        failed.write(OverviewMapHtmlAndJpegExportFailed);
        return;
    };
    let directory = export.directory.clone();
    let image = captured.image.clone();
    commands.entity(export.root).try_despawn();
    commands.entity(projection.parent()).try_despawn();
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
            let html_path = directory.join("overviewmap.html");
            atomically_write_and_sync_file(&html_path, html.as_bytes())?;
            Ok(html_path)
        }),
    ));
}

pub(super) fn complete_overview_map_html_and_jpeg_exports(
    mut tasks: Query<(Entity, &mut OverviewMapHtmlAndJpegWriteTask)>,
    mut commands: Commands,
    mut completed: MessageWriter<HtmlExportWritten>,
    mut failed: MessageWriter<OverviewMapHtmlAndJpegExportFailed>,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = block_on(poll_once(&mut task.0)) else {
            continue;
        };
        if let Ok(html_path) = result {
            completed.write(HtmlExportWritten(html_path));
        } else {
            failed.write(OverviewMapHtmlAndJpegExportFailed);
        }
        commands.entity(entity).despawn();
    }
}
