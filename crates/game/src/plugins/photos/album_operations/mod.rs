//! Queries and mutations used to operate on photo albums and their photos.

use bevy::prelude::*;

use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::photo_album_types::{
    ActivePhotoAlbum, AlbumMember, DeletePhotoRequest, PhotoAlbum, PhotoAlbumCapacityPages,
    PhotoAlbumOrder, PhotoAlbumPage, PhotoCameraRoll, PhotoMove, SelectedAlbumPhoto,
    PHOTO_ALBUM_PAGE_SIZE, PHOTO_CAMERA_ROLL_CAPACITY,
};
use super::photo_capture_types::Photo;

pub(super) fn find_photo_camera_roll_album(
    albums: &Query<(
        Entity,
        &PhotoAlbum,
        Option<&WorldMember>,
        Option<&PhotoAlbumPage>,
        Option<&SelectedAlbumPhoto>,
        Option<&PhotoMove>,
        Option<&PhotoCameraRoll>,
    )>,
) -> Option<Entity> {
    albums
        .iter()
        .find(|row| row.6.is_some())
        .or_else(|| albums.iter().min_by_key(|row| row.0.to_bits()))
        .map(|row| row.0)
}

pub(super) fn change_active_photo_album_page_by_signed_offset(
    album: Option<Entity>,
    delta: i32,
    albums: &Query<(
        Entity,
        &PhotoAlbum,
        Option<&WorldMember>,
        Option<&PhotoAlbumPage>,
        Option<&SelectedAlbumPhoto>,
        Option<&PhotoMove>,
        Option<&PhotoCameraRoll>,
    )>,
    capacities: &Query<&PhotoAlbumCapacityPages, With<PhotoAlbum>>,
    photos: &Query<(Entity, &Photo, &AlbumMember, Option<&PhotoAlbumOrder>)>,
    commands: &mut Commands,
) {
    if let Some(album) = album {
        let page = albums
            .get(album)
            .ok()
            .and_then(|row| row.3)
            .map_or(0, |page| page.0);
        let count = photos
            .iter()
            .filter(|(_, _, member, _)| member.0 == album)
            .count() as u32;
        let content_last = count.saturating_sub(1) / PHOTO_ALBUM_PAGE_SIZE;
        let capacity_last = capacities
            .get(album)
            .map_or(0, |capacity| capacity.0.saturating_sub(1));
        let last = content_last.max(capacity_last);
        commands
            .entity(album)
            .insert(PhotoAlbumPage(page.saturating_add_signed(delta).min(last)));
    }
}

pub(super) fn find_photo_in_album_display_slot(
    album: Option<Entity>,
    slot: i32,
    albums: &Query<(
        Entity,
        &PhotoAlbum,
        Option<&WorldMember>,
        Option<&PhotoAlbumPage>,
        Option<&SelectedAlbumPhoto>,
        Option<&PhotoMove>,
        Option<&PhotoCameraRoll>,
    )>,
    photos: &Query<(Entity, &Photo, &AlbumMember, Option<&PhotoAlbumOrder>)>,
) -> Option<Entity> {
    let album = album?;
    let album_state = albums.get(album).ok()?;
    let rank = photo_rank_for_album_display_slot(slot, album_state.6.is_some(), album_state.3)?;
    ordered_photo_entities_in_album(album, photos)
        .get(rank)
        .copied()
}

pub(super) fn photo_rank_for_album_display_slot(
    slot: i32,
    camera_roll: bool,
    page: Option<&PhotoAlbumPage>,
) -> Option<usize> {
    let slot = u32::try_from(slot).ok()?;
    let capacity = if camera_roll {
        PHOTO_CAMERA_ROLL_CAPACITY
    } else {
        PHOTO_ALBUM_PAGE_SIZE
    };
    if slot >= capacity {
        return None;
    }
    let page = if camera_roll {
        0
    } else {
        page.map_or(0, |page| page.0)
    };
    page.checked_mul(PHOTO_ALBUM_PAGE_SIZE)?
        .checked_add(slot)
        .map(|rank| rank as usize)
}

pub(super) fn ordered_photo_entities_in_album(
    album: Entity,
    photos: &Query<(Entity, &Photo, &AlbumMember, Option<&PhotoAlbumOrder>)>,
) -> Vec<Entity> {
    let mut entries: Vec<_> = photos
        .iter()
        .filter(|(_, _, member, _)| member.0 == album)
        .map(|(entity, photo, _, order)| {
            (
                (
                    order.map_or(photo.captured_tick, |order| order.0),
                    entity.to_bits(),
                ),
                entity,
            )
        })
        .collect();
    entries.sort_unstable_by_key(|entry| entry.0);
    entries.into_iter().map(|entry| entry.1).collect()
}

pub(super) fn ordered_photo_album_entities(albums: impl Iterator<Item = Entity>) -> Vec<Entity> {
    let mut albums: Vec<_> = albums.collect();
    albums.sort_unstable_by_key(|album| album.to_bits());
    albums
}

#[cfg(test)]
mod camera_roll_slot_tests {
    use super::*;

    #[test]
    fn camera_roll_exposes_all_twenty_four_photos_without_album_pagination() {
        let mut world = World::new();
        let roll = world
            .spawn((
                PhotoAlbum { profile: default() },
                PhotoCameraRoll,
                PhotoAlbumPage(4),
            ))
            .id();
        let photos: Vec<_> = (0..24)
            .map(|index| {
                world
                    .spawn((
                        Photo {
                            image: default(),
                            captured_tick: index,
                            camera_position: Vec3::ZERO,
                            camera_rotation: Quat::IDENTITY,
                            score_milli: 0,
                        },
                        AlbumMember(roll),
                        PhotoAlbumOrder(index),
                    ))
                    .id()
            })
            .collect();
        let mut state = bevy::ecs::system::SystemState::<(
            Query<(
                Entity,
                &PhotoAlbum,
                Option<&WorldMember>,
                Option<&PhotoAlbumPage>,
                Option<&SelectedAlbumPhoto>,
                Option<&PhotoMove>,
                Option<&PhotoCameraRoll>,
            )>,
            Query<(Entity, &Photo, &AlbumMember, Option<&PhotoAlbumOrder>)>,
        )>::new(&mut world);
        let (albums, entries) = state.get(&world).unwrap();
        assert_eq!(
            find_photo_in_album_display_slot(Some(roll), 0, &albums, &entries),
            Some(photos[0])
        );
        assert_eq!(
            find_photo_in_album_display_slot(Some(roll), 23, &albums, &entries),
            Some(photos[23])
        );
        assert_eq!(
            find_photo_in_album_display_slot(Some(roll), 24, &albums, &entries),
            None
        );
    }
}

pub(super) fn select_photo_in_album_display_slot(
    album: Option<Entity>,
    slot: i32,
    albums: &Query<(
        Entity,
        &PhotoAlbum,
        Option<&WorldMember>,
        Option<&PhotoAlbumPage>,
        Option<&SelectedAlbumPhoto>,
        Option<&PhotoMove>,
        Option<&PhotoCameraRoll>,
    )>,
    photos: &Query<(Entity, &Photo, &AlbumMember, Option<&PhotoAlbumOrder>)>,
    commands: &mut Commands,
) {
    if let (Some(album), Some(photo)) = (
        album,
        find_photo_in_album_display_slot(album, slot, albums, photos),
    ) {
        for row in albums {
            if row.0 != album {
                commands.entity(row.0).remove::<SelectedAlbumPhoto>();
            }
        }
        commands.entity(album).insert(SelectedAlbumPhoto(photo));
    }
}

pub(super) fn deselect_photo_in_album_display_slot(
    album: Option<Entity>,
    slot: i32,
    albums: &Query<(
        Entity,
        &PhotoAlbum,
        Option<&WorldMember>,
        Option<&PhotoAlbumPage>,
        Option<&SelectedAlbumPhoto>,
        Option<&PhotoMove>,
        Option<&PhotoCameraRoll>,
    )>,
    photos: &Query<(Entity, &Photo, &AlbumMember, Option<&PhotoAlbumOrder>)>,
    commands: &mut Commands,
) {
    if let (Some(album), Some(photo)) = (
        album,
        find_photo_in_album_display_slot(album, slot, albums, photos),
    ) {
        if albums
            .get(album)
            .ok()
            .and_then(|row| row.4)
            .is_some_and(|selected| selected.0 == photo)
        {
            commands.entity(album).remove::<SelectedAlbumPhoto>();
        }
    }
}

pub(super) fn select_photo_album_by_list_index(
    index: i32,
    albums: &Query<(
        Entity,
        &PhotoAlbum,
        Option<&WorldMember>,
        Option<&PhotoAlbumPage>,
        Option<&SelectedAlbumPhoto>,
        Option<&PhotoMove>,
        Option<&PhotoCameraRoll>,
    )>,
    commands: &mut Commands,
) {
    if let Some(selected) = usize::try_from(index).ok().and_then(|index| {
        ordered_photo_album_entities(albums.iter().filter(|row| row.6.is_none()).map(|row| row.0))
            .get(index)
            .copied()
    }) {
        albums.iter().for_each(|row| {
            commands.entity(row.0).remove::<ActivePhotoAlbum>();
        });
        commands.entity(selected).insert(ActivePhotoAlbum);
    }
}

pub(super) fn request_deletion_of_selected_photo_from_album(
    album: Option<Entity>,
    albums: &Query<(
        Entity,
        &PhotoAlbum,
        Option<&WorldMember>,
        Option<&PhotoAlbumPage>,
        Option<&SelectedAlbumPhoto>,
        Option<&PhotoMove>,
        Option<&PhotoCameraRoll>,
    )>,
    deletions: &mut MessageWriter<DeletePhotoRequest>,
) {
    if let Some(photo) = album
        .and_then(|album| albums.get(album).ok())
        .and_then(|row| row.4)
        .map(|selected| selected.0)
    {
        deletions.write(DeletePhotoRequest { photo });
    }
}

pub(super) fn request_deletion_of_selected_photo_from_any_album(
    albums: &Query<(
        Entity,
        &PhotoAlbum,
        Option<&WorldMember>,
        Option<&PhotoAlbumPage>,
        Option<&SelectedAlbumPhoto>,
        Option<&PhotoMove>,
        Option<&PhotoCameraRoll>,
    )>,
    deletions: &mut MessageWriter<DeletePhotoRequest>,
) {
    if let Some(photo) = albums
        .iter()
        .find_map(|row| row.4.map(|selected| selected.0))
    {
        deletions.write(DeletePhotoRequest { photo });
    }
}

pub(super) fn reorder_photos_for_insertion_at_album_position(
    album: Entity,
    moving: Entity,
    target: u32,
    photos: &Query<(Entity, &Photo, &AlbumMember, Option<&PhotoAlbumOrder>)>,
    commands: &mut Commands,
) {
    for (rank, entity) in ordered_photo_entities_in_album(album, photos)
        .into_iter()
        .filter(|entity| *entity != moving)
        .enumerate()
    {
        let rank = rank as u64;
        commands
            .entity(entity)
            .insert(PhotoAlbumOrder(if rank >= u64::from(target) {
                rank + 1
            } else {
                rank
            }));
    }
}
