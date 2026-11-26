use bevy::prelude::*;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestGenerationPolicy;

pub(super) fn arrival_roll_passes(roll: u16, probability_per_thousand: i32) -> bool {
    i32::from(roll % 1000) <= probability_per_thousand
}

pub(super) fn generation_delay_ns(bounds_ns: [u64; 2], draw: impl FnOnce() -> u16) -> u64 {
    const TENTH_SECOND_NS: u64 = 100_000_000;
    let [minimum, maximum] = bounds_ns;
    let slots = maximum.saturating_sub(minimum) / TENTH_SECOND_NS;
    let offset = if slots == 0 {
        0
    } else {
        u64::from(draw()) % slots
    };
    minimum + offset * TENTH_SECOND_NS
}

pub(super) fn guest_spawn_probability(policy: &GuestGenerationPolicy, half_stars: u8) -> i32 {
    let fame_adjustment = f32::from(half_stars)
        * f32::from(policy.fame_rate_increase_permille_per_half_star)
        / 1000.0;
    (1000.0
        * (policy.base_spawn_probability + fame_adjustment)
            .max(policy.minimum_spawn_rate)
            .min(1.0)) as i32
}

pub(super) fn guest_population_cap(policy: &GuestGenerationPolicy, half_stars: u8) -> u32 {
    policy.maximum_guests_at_fame(half_stars.saturating_sub(1))
}

pub(super) fn displaced_from_entrance_gate(
    entrance: Vec3,
    gate_rotation: Quat,
    offset_m: [f32; 2],
) -> Vec3 {
    // Source ground (x, y) is Bevy (x, -z) in the gate's local frame.
    entrance + gate_rotation * Vec3::new(offset_m[0], 0.0, -offset_m[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_probability_latch_accepts_only_zero() {
        assert!(arrival_roll_passes(0, 0));
        assert!(!arrival_roll_passes(1, 0));
    }

    #[test]
    fn delay_uses_exclusive_tenth_second_slots() {
        assert_eq!(
            generation_delay_ns([1_000_000_000, 6_000_000_000], || 49),
            5_900_000_000
        );
        assert_eq!(
            generation_delay_ns([2_000_000_000; 2], || panic!("constant delay drew")),
            2_000_000_000
        );
    }
}
