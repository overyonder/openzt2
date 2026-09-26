//! Food and drink container quantity updates.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// Native default `CapMax` expressed in the shared Q16 authored-point unit.
pub(crate) const AUTHORED_CONTAINER_CAPACITY_Q16: i32 = 100 << 16;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FoodContainer {
    pub(crate) food: AssetId,
    pub(crate) amount_q16: i32,
    pub(crate) capacity_q16: i32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DrinkContainer {
    pub(crate) drink: AssetId,
    pub(crate) amount_q16: i32,
    pub(crate) capacity_q16: i32,
}

/// Applies one authored additive FoodLevel write without converting units.
/// The caller decides whether the target is valid; a valid saturated write is
/// still successful and returns the unchanged clamped amount.
pub(crate) fn apply_authored_container_quantity_write(
    amount_q16: i32,
    capacity_q16: i32,
    delta_q16: i32,
) -> i32 {
    i64::from(amount_q16)
        .saturating_add(i64::from(delta_q16))
        .clamp(0, i64::from(capacity_q16)) as i32
}
