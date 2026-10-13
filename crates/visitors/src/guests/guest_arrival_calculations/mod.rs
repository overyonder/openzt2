use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use bevy::prelude::*;
use openzt2_zoo_state::locomotion::locomotion_types::SpatialGrid;
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

/// Where a guest appears: displaced from the entrance gate when guests spawn
/// at the entrance, otherwise at the authored world emitter. `None` while the
/// map's extent is not yet known.
pub(super) fn guest_arrival_position(
    policy: &GuestGenerationPolicy,
    entrance: Vec3,
    gate_rotation: Quat,
    grid: &SpatialGrid,
    rng: &mut DeterministicRng,
) -> Option<Vec3> {
    let base = if policy.spawn_at_entrance {
        displaced_from_entrance_gate(entrance, gate_rotation, policy.arrival_offset_m, grid)?
    } else {
        // Source ground (x, y) is Bevy (x, -z).
        Vec3::new(
            policy.default_emitter_m[0],
            entrance.y,
            -policy.default_emitter_m[1],
        )
    };
    if !policy.stagger || policy.stagger_square_m <= 0.0 {
        return Some(base);
    }
    let width = policy.stagger_square_m;
    Some(
        base + Vec3::new(
            (random_unit(rng) - 0.5) * width,
            0.0,
            (random_unit(rng) - 0.5) * width,
        ),
    )
}

/// Applies an authored guest-manager displacement, in metres along the gate's
/// own source x and y axes, to the entrance anchor.
///
/// The authored arrival displacement (100 m out from the gate) can leave the
/// map, where nothing can walk. The original's world extends past the playable
/// map; ours ends at the navigation grid, so the point is pulled back along the
/// displacement to the last point inside the grid, half a cell in from its
/// edge so it lies within a cell. Guests still appear outside the gate and walk
/// in. `None` while the grid is not yet built.
pub(super) fn displaced_from_entrance_gate(
    entrance: Vec3,
    gate_rotation: Quat,
    offset_m: [f32; 2],
    grid: &SpatialGrid,
) -> Option<Vec3> {
    if grid.width == 0 || grid.height == 0 {
        return None;
    }
    // Source ground (x, y) is Bevy (x, -z) in the gate's local frame.
    let displacement = gate_rotation * Vec3::new(offset_m[0], 0.0, -offset_m[1]);
    let inset = grid.cell_size_m * 0.5;
    let minimum = grid.origin + Vec2::splat(inset);
    let maximum = grid.origin + Vec2::new(grid.width as f32, grid.height as f32) * grid.cell_size_m
        - Vec2::splat(inset);
    let start = entrance.xz();
    let delta = displacement.xz();
    // Largest fraction of the displacement that stays inside the inset grid.
    let mut reach = 1.0_f32;
    for axis in 0..2 {
        if delta[axis] > 0.0 {
            reach = reach.min((maximum[axis] - start[axis]) / delta[axis]);
        } else if delta[axis] < 0.0 {
            reach = reach.min((minimum[axis] - start[axis]) / delta[axis]);
        }
    }
    Some(entrance + displacement * reach.clamp(0.0, 1.0))
}

pub(super) fn random_unit(rng: &mut DeterministicRng) -> f32 {
    rng.next_u32() as f32 / u32::MAX as f32
}
