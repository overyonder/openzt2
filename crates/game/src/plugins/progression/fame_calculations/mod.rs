use super::fame_types::Fame;

pub(crate) fn update_current_and_maximum_fame(fame: &mut Fame, next_half_stars: u8) -> bool {
    let previous_fame = *fame;
    fame.half_stars = next_half_stars;
    fame.maximum_reached = fame.maximum_reached.max(next_half_stars);
    *fame != previous_fame
}

pub(crate) fn select_fame_level_for_rating_and_guest_count(
    thresholds: impl Iterator<Item = (u16, u16, u32)>,
    rating_permille: u16,
    guest_count: u32,
) -> u8 {
    thresholds
        .filter(|(_, minimum_rating, minimum_guests)| {
            rating_permille >= *minimum_rating && guest_count >= *minimum_guests
        })
        .map(|(level, _, _)| level.min(u16::from(u8::MAX)) as u8)
        .max()
        .unwrap_or(0)
}
