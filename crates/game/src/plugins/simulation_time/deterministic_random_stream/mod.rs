use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::world_spawn::persistent_id_types::PersistentId;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub(crate) struct ZooSeed(pub(crate) u64);

pub(crate) fn derive_zoo_seed_from_scenario_and_profile(
    scenario_identifier: AssetId,
    profile_identifier: AssetId,
) -> ZooSeed {
    let mut left_bytes = [0_u8; 8];
    let mut right_bytes = [0_u8; 8];
    for byte_index in 0..8 {
        left_bytes[byte_index] =
            scenario_identifier.0[byte_index] ^ profile_identifier.0[byte_index + 8];
        right_bytes[byte_index] =
            scenario_identifier.0[byte_index + 8] ^ profile_identifier.0[byte_index];
    }
    let left = u64::from_le_bytes(left_bytes);
    let right = u64::from_le_bytes(right_bytes);
    let mixed_identifiers = left ^ right.rotate_left(29) ^ 0x6f70_656e_7a74_3200;
    ZooSeed(mixed_identifiers ^ (mixed_identifiers >> 30).wrapping_mul(0xbf58_476d_1ce4_e5b9))
}

/// Stable stream discriminants. Changing these values changes saved simulation
/// sequences and therefore requires an explicit save-schema migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub(crate) enum RngDomain {
    Reproduction = 0,
    Behavior = 1,
    #[cfg(test)]
    Disease = 2,
    Guest = 3,
    Donation = 4,
    Fossil = 5,
    BiomeAutomaticPlacement = 6,
    Weather = 7,
    AmbientSpawner = 8,
    Challenge = 9,
    AnimalNeeds = 10,
    AnimalAdoptionOffers = 11,
    Research = 12,
}

/// A small PCG-XSH-RR stream. Components in each stochastic domain own their
/// instance; this value never lives in a shared random-service resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct DeterministicRng {
    state: u64,
    stream: u64,
}

impl DeterministicRng {
    const MULTIPLIER: u64 = 6_364_136_223_846_793_005;

    #[inline]
    pub(crate) fn from_entity(
        seed: ZooSeed,
        persistent_id: PersistentId,
        domain: RngDomain,
    ) -> Self {
        let identity =
            mix_u64_for_deterministic_random_stream(seed.0 ^ persistent_id.0.rotate_left(23));
        let stream = mix_u64_for_deterministic_random_stream(
            seed.0.rotate_right(17) ^ persistent_id.0 ^ domain as u64,
        ) | 1;
        let mut random_stream = Self { state: 0, stream };
        let _ = random_stream.next_u32();
        random_stream.state = random_stream.state.wrapping_add(identity);
        let _ = random_stream.next_u32();
        random_stream
    }

    #[inline]
    pub const fn from_raw(raw_state_and_stream: [u64; 2]) -> Self {
        Self {
            state: raw_state_and_stream[0],
            stream: raw_state_and_stream[1],
        }
    }

    #[inline]
    pub const fn to_raw(self) -> [u64; 2] {
        [self.state, self.stream]
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let previous_state = self.state;
        self.state = previous_state
            .wrapping_mul(Self::MULTIPLIER)
            .wrapping_add(self.stream);
        let xor_shifted = (((previous_state >> 18) ^ previous_state) >> 27) as u32;
        xor_shifted.rotate_right((previous_state >> 59) as u32)
    }

    #[inline]
    pub fn unit_f32(&mut self) -> f32 {
        const U24_TO_UNIT_F32_SCALE: f32 = 1.0 / 16_777_216.0;
        ((self.next_u32() >> 8) as f32) * U24_TO_UNIT_F32_SCALE
    }

    #[inline]
    pub fn range_u32(&mut self, upper_exclusive: u32) -> Option<u32> {
        if upper_exclusive == 0 {
            return None;
        }
        let rejection_threshold = upper_exclusive.wrapping_neg() % upper_exclusive;
        loop {
            let value = self.next_u32();
            if value >= rejection_threshold {
                return Some(value % upper_exclusive);
            }
        }
    }
}

#[inline]
const fn mix_u64_for_deterministic_random_stream(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
