use bevy::prelude::*;

use super::photo_album_types::{AlbumMember, DeletePhotoRequest};
use super::photo_capture_types::{Photo, PhotoSubjects};

fn create_test_entity_from_raw_bits(bits: u64) -> Entity {
    Entity::from_bits(bits)
}

#[test]
fn subjects_are_sorted_deduplicated_without_a_four_subject_capture_limit() {
    let mut subjects = PhotoSubjects::default();
    assert!(subjects.insert(create_test_entity_from_raw_bits(3)));
    assert!(subjects.insert(create_test_entity_from_raw_bits(1)));
    assert!(!subjects.insert(create_test_entity_from_raw_bits(3)));
    assert!(subjects.insert(create_test_entity_from_raw_bits(2)));
    assert_eq!(
        subjects.0.iter().map(|id| id.to_bits()).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert!(subjects.insert(create_test_entity_from_raw_bits(4)));
    assert!(subjects.insert(create_test_entity_from_raw_bits(5)));
    assert_eq!(subjects.0.len(), 5);
}

#[test]
fn deleting_an_album_entry_releases_image_and_entity_ownership() {
    let mut app = App::new();
    app.init_resource::<Assets<Image>>()
        .add_message::<DeletePhotoRequest>()
        .add_message::<
            crate::plugins::persistence::generated_photo_persistence_types::
                DeleteGeneratedPhotoImageAndChallengeCopies,
        >()
        .add_systems(
            Update,
            super::photo_album_storage::delete_requested_photos_and_release_unpersisted_images,
        );
    let album = app.world_mut().spawn_empty().id();
    let image = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::default());
    let photo = app
        .world_mut()
        .spawn((
            Photo {
                image: image.clone(),
                captured_tick: 1,
                camera_position: Vec3::ZERO,
                camera_rotation: Quat::IDENTITY,
                score_milli: 0,
            },
            AlbumMember(album),
        ))
        .id();
    app.world_mut().write_message(DeletePhotoRequest { photo });
    app.update();
    assert!(app.world().get_entity(photo).is_err());
    assert!(app
        .world()
        .resource::<Assets<Image>>()
        .get(&image)
        .is_none());
    assert_eq!(
        app.world()
            .resource::<Messages<
                crate::plugins::persistence::generated_photo_persistence_types::
                    DeleteGeneratedPhotoImageAndChallengeCopies,
            >>()
            .len(),
        1
    );
}
