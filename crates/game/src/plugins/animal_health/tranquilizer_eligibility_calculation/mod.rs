use openzt2_game_data::world_definitions::animal_health::TranquilizerEligibility;

pub(crate) fn authored_tranquilizer_allows_animal_state(
    eligibility: TranquilizerEligibility,
    escaped: bool,
    rampaging: bool,
) -> bool {
    (escaped && eligibility.contains_all(TranquilizerEligibility::ESCAPED))
        || (rampaging && eligibility.contains_all(TranquilizerEligibility::RAMPAGING))
        || (!escaped && !rampaging && eligibility.contains_all(TranquilizerEligibility::CONTAINED))
}
