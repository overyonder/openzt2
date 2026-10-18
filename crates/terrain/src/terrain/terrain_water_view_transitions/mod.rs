use bevy::prelude::*;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterRegionFade;

pub(super) fn advance_terrain_water_region_fades(
    time: Res<Time>,
    phase: Res<State<GamePhase>>,
    mut regions: Query<&mut TerrainWaterRegionFade>,
) {
    if *phase.get() != GamePhase::InGame {
        return;
    }
    for mut fade in &mut regions {
        fade.advance(time.delta_secs());
    }
}
