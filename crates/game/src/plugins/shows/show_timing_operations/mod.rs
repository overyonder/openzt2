use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

#[inline]
pub(super) fn convert_authored_nanoseconds_to_fixed_simulation_ticks(
    authored_nanoseconds: u64,
    world_definitions: WorldDefinitionsView<'_>,
) -> u32 {
    let fixed_simulation_hertz = u64::from(world_definitions.timing().fixed_hz);
    authored_nanoseconds
        .saturating_mul(fixed_simulation_hertz)
        .saturating_add(999_999_999)
        .checked_div(1_000_000_000)
        .unwrap_or(0)
        .min(u64::from(u32::MAX)) as u32
}
