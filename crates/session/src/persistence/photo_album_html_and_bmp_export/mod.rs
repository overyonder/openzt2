//! Explicit asynchronous export of one album to a self-contained HTML directory.

use std::{io, path::PathBuf};

use bevy::{
    ecs::relationship::RelationshipTarget,
    prelude::*,
    tasks::{block_on, poll_once, IoTaskPool, Task},
};

use openzt2_interface_state::photos::photo_album_types::ExportPhotoAlbum;
use openzt2_interface_state::photos::photo_album_types::PhotoAlbumOrder;
use openzt2_interface_state::photos::photo_album_types::{AlbumPhotos, PhotoAlbum};
use openzt2_interface_state::photos::photo_capture_types::Photo;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;

use openzt2_interface_state::persistence::profile_types::ProfileOptions;

use super::{
    durable_filesystem_operations::atomically_write_and_sync_file,
    html_export_result_opening::HtmlExportWritten,
    persistence_filesystem_paths::PersistenceFilesystemPaths,
    rgba8_bmp_encoding::encode_tightly_packed_rgba8_as_bgra_bmp,
};

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PhotoAlbumHtmlAndBmpExportFailed {
    pub album: Entity,
}

#[derive(Component)]
pub(super) struct PhotoAlbumHtmlAndBmpWriteTask {
    album: Entity,
    task: Task<io::Result<PathBuf>>,
}

pub(super) fn begin_photo_album_html_and_bmp_exports(
    mut requests: MessageReader<ExportPhotoAlbum>,
    directories: Option<Res<PersistenceFilesystemPaths>>,
    profile: Res<ProfileOptions>,
    albums: Query<(&PersistentId, Option<&AlbumPhotos>), With<PhotoAlbum>>,
    photos: Query<(
        &PersistentId,
        &Photo,
        Option<&PhotoAlbumOrder>,
        Option<&openzt2_photos::PhotoCaption>,
    )>,
    images: Res<Assets<Image>>,
    mut commands: Commands,
    mut failed: MessageWriter<PhotoAlbumHtmlAndBmpExportFailed>,
) {
    for request in requests.read() {
        let Some(directories) = directories.as_deref() else {
            failed.write(PhotoAlbumHtmlAndBmpExportFailed {
                album: request.album,
            });
            continue;
        };
        let Ok((album_id, members)) = albums.get(request.album) else {
            failed.write(PhotoAlbumHtmlAndBmpExportFailed {
                album: request.album,
            });
            continue;
        };
        let mut entries = members
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .filter_map(|entity| {
                let (id, photo, order, caption) = photos.get(entity).ok()?;
                let image = images.get(&photo.image)?;
                let data = image.data.as_deref()?;
                let size = image.texture_descriptor.size;
                encode_tightly_packed_rgba8_as_bgra_bmp(size.width, size.height, data)
                    .ok()
                    .map(|bytes| {
                        (
                            order.map_or(photo.captured_tick, |order| order.0),
                            id.0,
                            caption.map(|caption| caption.0.clone()),
                            bytes,
                        )
                    })
            })
            .collect::<Vec<_>>();
        entries.sort_unstable_by_key(|entry| (entry.0, entry.1));
        let directory = directories
            .photo_album_export_directory_path(&profile.profile_identifier.0, album_id.0);
        let html = create_photo_album_html_document(&entries);
        let album = request.album;
        commands.spawn(PhotoAlbumHtmlAndBmpWriteTask {
            album,
            task: IoTaskPool::get().spawn(async move {
                entries.into_iter().try_for_each(|(_, id, _, bytes)| {
                    atomically_write_and_sync_file(
                        &directory.join(format!("photo-{id:016x}.bmp")),
                        &bytes,
                    )
                })?;
                let html_path = directory.join("album.html");
                atomically_write_and_sync_file(&html_path, html.as_bytes())?;
                Ok(html_path)
            }),
        });
    }
}

pub(super) fn complete_photo_album_html_and_bmp_exports(
    mut tasks: Query<(Entity, &mut PhotoAlbumHtmlAndBmpWriteTask)>,
    mut commands: Commands,
    mut completed: MessageWriter<HtmlExportWritten>,
    mut failed: MessageWriter<PhotoAlbumHtmlAndBmpExportFailed>,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = block_on(poll_once(&mut task.task)) else {
            continue;
        };
        if let Ok(html_path) = result {
            completed.write(HtmlExportWritten(html_path));
        } else {
            failed.write(PhotoAlbumHtmlAndBmpExportFailed { album: task.album });
        }
        commands.entity(entity).despawn();
    }
}

fn create_photo_album_html_document(
    photo_entries: &[(u64, u64, Option<String>, Vec<u8>)],
) -> String {
    let mut table_rows = String::new();
    for (photo_index, (_, persistent_photo_identifier, caption, _)) in
        photo_entries.iter().enumerate()
    {
        if photo_index % 4 == 0 {
            table_rows.push_str("<tr>");
        }
        table_rows.push_str(&format!(
            "<td><a href=\"photo-{persistent_photo_identifier:016x}.bmp\"><img src=\"photo-{persistent_photo_identifier:016x}.bmp\" width=\"198\" height=\"150\"></a>"
        ));
        if let Some(caption) = caption {
            table_rows.push_str("<p>");
            for character in caption.chars() {
                match character {
                    '&' => table_rows.push_str("&amp;"),
                    '<' => table_rows.push_str("&lt;"),
                    '>' => table_rows.push_str("&gt;"),
                    character => table_rows.push(character),
                }
            }
            table_rows.push_str("</p>");
        }
        table_rows.push_str("</td>");
        if photo_index % 4 == 3 {
            table_rows.push_str("</tr>\n");
        }
    }
    if photo_entries.len() % 4 != 0 {
        table_rows.push_str("</tr>\n");
    }
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Photo Album</title></head><body><table>{table_rows}</table></body></html>\n"
    )
}
