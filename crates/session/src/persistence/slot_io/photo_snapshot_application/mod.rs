use bevy::{
    asset::RenderAssetUsages,
    prelude::{Assets, Commands, DetectChangesMut, Entity, Image, Query},
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use openzt2_interface_state::photos::photo_album_types::AlbumMember;
use openzt2_interface_state::photos::photo_album_types::PhotoAlbumOrder;
use openzt2_interface_state::photos::photo_capture_types::Photo;
use openzt2_interface_state::photos::photo_capture_types::PhotoSubjects;
use openzt2_interface_state::photos::photo_challenge_types::PhotoChallengeCatalogueIndex;
use openzt2_interface_state::photos::photo_challenge_types::PhotoChallengeProgress;
use openzt2_world_state::world_spawn::persistent_id_index::PersistentIdIndex;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_interface_state::persistence::generated_photo_persistence_types::DurableChallengePhotoFileIdentifiers;

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    photo_snapshot_types::{PhotoChallengeProgressSnapshotRecord, PhotoSnapshotRecord},
};

pub(super) fn spawn_snapshot_photos(
    commands: &mut Commands,
    photo_snapshot_records: Vec<PhotoSnapshotRecord>,
    world_root_entity: Entity,
    identities: &PersistentIdIndex,
    persistent_identifier_allocator: &mut PersistentIdAllocator,
    loaded_photo_images: &mut Assets<Image>,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    for photo_snapshot_record in photo_snapshot_records {
        let album_entity = identities
            .get(
                world_root_entity,
                photo_snapshot_record.album_persistent_identifier,
            )
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;

        persistent_identifier_allocator
            .reserve_imported(photo_snapshot_record.persistent_identifier)
            .map_err(|_| WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier)?;

        let mut photo_image = Image::new_uninit(
            Extent3d {
                width: photo_snapshot_record.image_width_pixels,
                height: photo_snapshot_record.image_height_pixels,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        photo_image.data = Some(photo_snapshot_record.rgba8_pixel_bytes);
        let photo_image_handle = loaded_photo_images.add(photo_image);
        commands.spawn((
            photo_snapshot_record.persistent_identifier,
            WorldMember {
                root: world_root_entity,
            },
            Photo {
                image: photo_image_handle,
                captured_tick: photo_snapshot_record.captured_simulation_tick,
                camera_position: photo_snapshot_record.camera_world_position,
                camera_rotation: photo_snapshot_record.camera_world_rotation,
                score_milli: photo_snapshot_record.score_millipoints,
            },
            PhotoSubjects::default(),
            AlbumMember(album_entity),
            PhotoAlbumOrder(photo_snapshot_record.album_order),
            DurableChallengePhotoFileIdentifiers {
                challenge_definition_identifiers: photo_snapshot_record
                    .challenge_photo_file_identifiers,
            },
        ));
    }
    Ok(())
}

pub(super) fn restore_snapshot_photo_challenge_completion(
    records: &[PhotoChallengeProgressSnapshotRecord],
    root: Entity,
    identities: &PersistentIdIndex,
    catalogue: Option<&PhotoChallengeCatalogueIndex>,
    live: &mut Query<&mut PhotoChallengeProgress>,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    use WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference as Missing;
    for record in records {
        let challenge = catalogue
            .and_then(|catalogue| catalogue.rules.get(&record.challenge_definition_identifier))
            .ok_or(Missing)?;
        let completed_by = record
            .completed_by_photo_persistent_identifier
            .map(|id| identities.get(root, id).ok_or(Missing))
            .transpose()?;
        live.get_mut(*challenge)
            .map_err(|_| Missing)?
            .set_if_neq(PhotoChallengeProgress {
                completed: record.completed,
                completed_by,
                rating_stars: record.rating_stars,
            });
    }
    Ok(())
}
