use bevy::{ecs::system::SystemState, prelude::*};
use openzt2_game_data::{
    world_scenario::{PhotoChallengeRule, WorldScenarioDocument},
    AssetId,
};

use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_interface_state::photos::photo_album_types::AlbumMember;
use openzt2_interface_state::photos::photo_album_types::PhotoAlbum;
use openzt2_interface_state::photos::photo_capture_types::Photo;
use openzt2_interface_state::photos::photo_challenge_types::PhotoChallenge;
use openzt2_interface_state::photos::photo_challenge_types::PhotoChallengeProgress;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::{
    super::{
        persistence_failure_types::WorldSnapshotPersistenceFailure,
        snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
    },
    photo_snapshot_application::apply_photo_and_challenge_progress_snapshot_records_to_live_world,
    photo_snapshot_section_encoding::{
        append_encoded_photo_snapshot_section, decode_and_validate_photo_snapshot_section,
    },
    photo_snapshot_types::{PhotoChallengeProgressSnapshotRecord, PhotoSnapshotRecord},
};

const FIRST_CHALLENGE: AssetId = AssetId([
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
]);
const SECOND_CHALLENGE: AssetId = AssetId([
    0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x8b, 0x8c, 0x8d, 0x8e, 0x8f,
]);

// Each record is tag 2, sixteen opaque identity bytes, a little-endian photo ID,
// and one rating byte. These bytes are independent of the encoder.
const CHALLENGE_PROGRESS_WIRE_BYTES: [u8; 52] = [
    2, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
    0x1f, 41, 0, 0, 0, 0, 0, 0, 0, 2, 2, 0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88,
    0x89, 0x8a, 0x8b, 0x8c, 0x8d, 0x8e, 0x8f, 42, 0, 0, 0, 0, 0, 0, 0, 5,
];

fn challenge_section(payload_byte_count: usize) -> WorldSnapshotSectionDirectoryEntry {
    WorldSnapshotSectionDirectoryEntry {
        encoded_record_count: 2,
        payload_start_byte_offset: 0,
        payload_byte_count: payload_byte_count as u64,
    }
}

#[test]
fn challenge_identity_codec_agrees_with_literal_wire_records() {
    let records = [
        PhotoChallengeProgressSnapshotRecord {
            challenge_definition_identifier: FIRST_CHALLENGE,
            completed_by_photo_persistent_identifier: Some(PersistentId(41)),
            rating_stars: 2,
        },
        PhotoChallengeProgressSnapshotRecord {
            challenge_definition_identifier: SECOND_CHALLENGE,
            completed_by_photo_persistent_identifier: Some(PersistentId(42)),
            rating_stars: 5,
        },
    ];
    let mut encoded = Vec::new();
    append_encoded_photo_snapshot_section(&mut encoded, &[], &records);
    assert_eq!(encoded, CHALLENGE_PROGRESS_WIRE_BYTES);

    let (photos, decoded) = decode_and_validate_photo_snapshot_section(
        &CHALLENGE_PROGRESS_WIRE_BYTES,
        challenge_section(52),
    )
    .unwrap();
    assert!(photos.is_empty());
    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].challenge_definition_identifier, FIRST_CHALLENGE);
    assert_eq!(
        decoded[0].completed_by_photo_persistent_identifier,
        Some(PersistentId(41))
    );
    assert_eq!(decoded[0].rating_stars, 2);
    assert_eq!(decoded[1].challenge_definition_identifier, SECOND_CHALLENGE);
    assert_eq!(
        decoded[1].completed_by_photo_persistent_identifier,
        Some(PersistentId(42))
    );
    assert_eq!(decoded[1].rating_stars, 5);
}

#[test]
fn foreign_wire_records_reject_duplicate_challenge_identity() {
    let duplicate_records = CHALLENGE_PROGRESS_WIRE_BYTES[..26].repeat(2);
    assert_eq!(
        decode_and_validate_photo_snapshot_section(&duplicate_records, challenge_section(52))
            .unwrap_err(),
        WorldSnapshotPersistenceFailure::CorruptSnapshotSection,
    );
}

#[test]
fn restored_challenges_with_the_same_local_row_keep_distinct_photo_completions() {
    let mut world = World::new();
    world.insert_resource(Assets::<Image>::default());
    let root = world.spawn_empty().id();
    world.insert_resource(PersistentIdAllocator::new(root));
    let album = world
        .spawn((
            PersistentId(7),
            PhotoAlbum {
                profile: AssetId([9; 16]),
            },
            WorldMember { root },
        ))
        .id();

    let mut scenario_assets = Assets::<WorldScenarioDocumentAsset>::default();
    let mut challenges = Vec::new();
    // Reverse live entity order so the first row-0 receiver is the wrong one
    // for the first wire record.
    for id in [SECOND_CHALLENGE, FIRST_CHALLENGE] {
        let source = scenario_assets.add(WorldScenarioDocumentAsset::from_test_document(
            WorldScenarioDocument {
                photo_challenges: vec![PhotoChallengeRule {
                    id,
                    instruction_key: AssetId([3; 16]),
                    completion_key: AssetId([4; 16]),
                    difficulty: 1,
                }],
                ..Default::default()
            },
        ));
        let entity = world
            .spawn((
                PhotoChallenge { id, source, row: 0 },
                PhotoChallengeProgress::default(),
                WorldMember { root },
            ))
            .id();
        challenges.push(entity);
    }

    let photos = [41, 42]
        .into_iter()
        .map(|id| PhotoSnapshotRecord {
            persistent_identifier: PersistentId(id),
            album_persistent_identifier: PersistentId(7),
            album_order: id,
            captured_simulation_tick: 123,
            camera_world_position: Vec3::new(1.0, 2.0, 3.0),
            camera_world_rotation: Quat::IDENTITY,
            score_millipoints: 2000,
            image_width_pixels: 1,
            image_height_pixels: 1,
            rgba8_pixel_bytes: vec![id as u8, 10, 20, 255],
            challenge_photo_file_identifiers: vec![],
        })
        .collect();
    let (_, saved_progress) = decode_and_validate_photo_snapshot_section(
        &CHALLENGE_PROGRESS_WIRE_BYTES,
        challenge_section(52),
    )
    .unwrap();
    let mut state = SystemState::<(
        Commands,
        Query<(Entity, &PersistentId)>,
        ResMut<PersistentIdAllocator>,
        ResMut<Assets<Image>>,
        Query<(&PhotoChallenge, &mut PhotoChallengeProgress)>,
    )>::new(&mut world);
    {
        let (mut commands, identifiers, mut allocator, mut images, mut live_progress) =
            state.get_mut(&mut world).unwrap();
        apply_photo_and_challenge_progress_snapshot_records_to_live_world(
            &mut commands,
            photos,
            saved_progress,
            root,
            &identifiers,
            &mut allocator,
            &mut images,
            &mut live_progress,
        )
        .unwrap();
    }
    state.apply(&mut world);

    for (entity, expected_id, photo_id, stars) in [
        (challenges[0], SECOND_CHALLENGE, 42, 5),
        (challenges[1], FIRST_CHALLENGE, 41, 2),
    ] {
        let challenge = world.get::<PhotoChallenge>(entity).unwrap();
        assert_eq!(challenge.id, expected_id);
        assert_eq!(challenge.row, 0);
        assert_eq!(
            scenario_assets
                .get(&challenge.source)
                .unwrap()
                .document
                .photo_challenges[0]
                .id,
            expected_id
        );
        let progress = world.get::<PhotoChallengeProgress>(entity).unwrap();
        assert!(progress.completed);
        assert_eq!(progress.rating_stars, stars);
        let photo_entity = progress.completed_by.unwrap();
        assert_eq!(
            world.get::<PersistentId>(photo_entity),
            Some(&PersistentId(photo_id))
        );
        assert_eq!(world.get::<AlbumMember>(photo_entity).unwrap().0, album);
        let photo = world.get::<Photo>(photo_entity).unwrap();
        assert_eq!(
            world
                .resource::<Assets<Image>>()
                .get(&photo.image)
                .unwrap()
                .data
                .as_deref(),
            Some([photo_id as u8, 10, 20, 255].as_slice())
        );
    }
}
