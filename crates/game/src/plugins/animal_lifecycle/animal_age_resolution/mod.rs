use openzt2_game_data::species::LifeStage;

pub(super) fn resolve_animal_life_stage_from_elapsed_age_ticks(
    age_ticks: u64,
    stage_start_ticks: [u64; 4],
) -> LifeStage {
    if age_ticks >= stage_start_ticks[3] {
        LifeStage::Elder
    } else if age_ticks >= stage_start_ticks[2] {
        LifeStage::Adult
    } else if age_ticks >= stage_start_ticks[1] {
        LifeStage::Young
    } else {
        LifeStage::Juvenile
    }
}
