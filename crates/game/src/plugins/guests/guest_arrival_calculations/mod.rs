use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;
use bevy::prelude::*;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestDefinition;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestGenerationPolicy;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestRarity;

pub(super) fn choose_guest_definition<'a>(
    definitions: WorldDefinitionsView<'a>,
    rarity: GuestRarityBucket,
    rng: &mut DeterministicRng,
) -> Option<&'a GuestDefinition> {
    [
        rarity,
        GuestRarityBucket::Common,
        GuestRarityBucket::Uncommon,
        GuestRarityBucket::Rare,
    ]
    .into_iter()
    .find_map(|rarity| {
        let matching = definitions
            .guest_definitions()
            .filter(|definition| guest_definition_matches_rarity(definition, rarity))
            .count();
        let selected = (matching != 0).then(|| rng.next_u32() as usize % matching)?;
        definitions
            .guest_definitions()
            .filter(|definition| guest_definition_matches_rarity(definition, rarity))
            .nth(selected)
    })
}

#[derive(Clone, Copy)]
pub(super) enum GuestRarityBucket {
    Common,
    Uncommon,
    Rare,
}

pub(super) fn choose_guest_rarity(
    thresholds: [u16; 2],
    rng: &mut DeterministicRng,
) -> GuestRarityBucket {
    let roll = (rng.next_u32() % 100) as u16;
    if roll < thresholds[0] {
        GuestRarityBucket::Rare
    } else if roll < thresholds[1] {
        GuestRarityBucket::Uncommon
    } else {
        GuestRarityBucket::Common
    }
}

fn guest_definition_matches_rarity(guest: &GuestDefinition, rarity: GuestRarityBucket) -> bool {
    matches!(
        (&guest.rarity, rarity),
        (GuestRarity::Common, GuestRarityBucket::Common)
            | (GuestRarity::Uncommon, GuestRarityBucket::Uncommon)
            | (GuestRarity::Rare, GuestRarityBucket::Rare)
    )
}

pub(super) fn generation_delay_ticks(
    asset: WorldDefinitionsView<'_>,
    policy: &GuestGenerationPolicy,
    rng: &mut DeterministicRng,
) -> u64 {
    let low = policy.delay_ns[0];
    let high = policy.delay_ns[1].max(low);
    let width = high.saturating_sub(low).saturating_add(1);
    let delay_ns = low.saturating_add(u64::from(rng.next_u32()) % width.max(1));
    let fixed_hz = u64::from(asset.timing().fixed_hz.max(1));
    delay_ns
        .saturating_mul(fixed_hz)
        .saturating_add(999_999_999)
        / 1_000_000_000
}

pub(super) fn guest_spawn_probability(
    policy: &GuestGenerationPolicy,
    fame_half_stars: u8,
    admission_cents: i64,
    species_count: u32,
) -> f32 {
    let fame_multiplier = 1.0
        + f32::from(policy.fame_rate_increase_permille_per_half_star)
            * 0.001
            * f32::from(fame_half_stars);
    let normal = i64::from(policy.normal_admission_cents[0]).max(1);
    let price_percent = (admission_cents - normal) as f32 * 100.0 / normal as f32;
    let price_adjustment = policy
        .price_adjustments
        .iter()
        .find(|row| {
            let bounds = row.percent_range;
            price_percent >= bounds[0] && price_percent <= bounds[1]
        })
        .map(|row| row.percent_adjustment + row.percent_delta * price_percent)
        .unwrap_or(0.0);
    let species_step = u32::from(policy.species_adjustment.species_per_adjustment).max(1);
    let species_steps = species_count / species_step;
    let species_adjustment = (species_steps as f32
        * policy.species_adjustment.percent_per_adjustment)
        .min(policy.species_adjustment.maximum_percent_adjustment);
    (policy.base_spawn_probability * fame_multiplier + price_adjustment + species_adjustment)
        .max(policy.minimum_spawn_rate)
        .clamp(0.0, 1.0)
}

pub(super) fn guest_arrival_position(
    policy: &GuestGenerationPolicy,
    entrance: Vec3,
    rng: &mut DeterministicRng,
) -> Vec3 {
    let base = if policy.spawn_at_entrance {
        entrance
            + Vec3::new(
                policy.arrival_offset_cm[0] as f32 * 0.01,
                0.0,
                policy.arrival_offset_cm[1] as f32 * 0.01,
            )
    } else {
        Vec3::new(
            policy.default_emitter_cm[0] as f32 * 0.01,
            entrance.y,
            policy.default_emitter_cm[1] as f32 * 0.01,
        )
    };
    if !policy.stagger || policy.stagger_square_cm == 0 {
        return base;
    }
    let width = policy.stagger_square_cm as f32 * 0.01;
    base + Vec3::new(
        (random_unit(rng) - 0.5) * width,
        0.0,
        (random_unit(rng) - 0.5) * width,
    )
}

pub(super) fn random_unit(rng: &mut DeterministicRng) -> f32 {
    rng.next_u32() as f32 / u32::MAX as f32
}
