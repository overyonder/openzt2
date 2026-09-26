use crate::plugins::world_spawn::persistent_id_types::PersistentId;

use super::deterministic_random_stream::{DeterministicRng, RngDomain, ZooSeed};

#[test]
fn entity_and_domain_streams_are_reproducible_and_independent() {
    let zoo_seed = ZooSeed(0x1234_5678_9abc_def0);
    let persistent_entity_identifier = PersistentId(42);
    let mut first_behavior_stream =
        DeterministicRng::from_entity(zoo_seed, persistent_entity_identifier, RngDomain::Behavior);
    let mut replayed_behavior_stream =
        DeterministicRng::from_entity(zoo_seed, persistent_entity_identifier, RngDomain::Behavior);
    let mut disease_stream =
        DeterministicRng::from_entity(zoo_seed, persistent_entity_identifier, RngDomain::Disease);
    let mut challenge_stream =
        DeterministicRng::from_entity(zoo_seed, persistent_entity_identifier, RngDomain::Challenge);
    let first_behavior_sequence = [
        first_behavior_stream.next_u32(),
        first_behavior_stream.next_u32(),
        first_behavior_stream.next_u32(),
    ];
    let replayed_behavior_sequence = [
        replayed_behavior_stream.next_u32(),
        replayed_behavior_stream.next_u32(),
        replayed_behavior_stream.next_u32(),
    ];
    let disease_sequence = [
        disease_stream.next_u32(),
        disease_stream.next_u32(),
        disease_stream.next_u32(),
    ];
    let challenge_sequence = [
        challenge_stream.next_u32(),
        challenge_stream.next_u32(),
        challenge_stream.next_u32(),
    ];
    assert_eq!(first_behavior_sequence, replayed_behavior_sequence);
    assert_eq!(
        first_behavior_sequence,
        [1_940_188_696, 4_216_940_908, 2_056_525_086]
    );
    assert_ne!(first_behavior_sequence, disease_sequence);
    assert_ne!(first_behavior_sequence, challenge_sequence);
    assert_ne!(disease_sequence, challenge_sequence);
    assert_eq!(std::mem::size_of::<DeterministicRng>(), 16);
}

#[test]
fn restored_raw_random_stream_state_continues_the_exact_sequence() {
    let mut uninterrupted_random_stream =
        DeterministicRng::from_entity(ZooSeed(9), PersistentId(77), RngDomain::Donation);
    for _ in 0..17 {
        let _ = uninterrupted_random_stream.next_u32();
    }
    let saved_random_stream_state = uninterrupted_random_stream.to_raw();
    let expected_continuation = [
        uninterrupted_random_stream.next_u32(),
        uninterrupted_random_stream.next_u32(),
        uninterrupted_random_stream.next_u32(),
    ];
    let mut restored_random_stream = DeterministicRng::from_raw(saved_random_stream_state);
    assert_eq!(
        expected_continuation,
        [
            restored_random_stream.next_u32(),
            restored_random_stream.next_u32(),
            restored_random_stream.next_u32(),
        ]
    );
    assert_eq!(
        restored_random_stream.to_raw(),
        uninterrupted_random_stream.to_raw()
    );
}

#[test]
fn bounded_integer_and_unit_interval_draws_respect_their_contracts() {
    let mut random_stream = DeterministicRng::from_raw([0xfeed_face_cafe_beef, 0x1235]);
    assert_eq!(random_stream.range_u32(0), None);
    let mut bounded_draw_buckets = [0_u32; 7];
    for _ in 0..70_000 {
        let bounded_draw = random_stream.range_u32(7).unwrap();
        bounded_draw_buckets[bounded_draw as usize] += 1;
        let unit_interval_draw = random_stream.unit_f32();
        assert!((0.0..1.0).contains(&unit_interval_draw));
    }
    assert!(bounded_draw_buckets
        .iter()
        .all(|draw_count| (9_500..10_500).contains(draw_count)));
}
