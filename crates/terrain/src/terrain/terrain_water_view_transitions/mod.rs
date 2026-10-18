use bevy::prelude::*;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterRegionFade;

pub(super) fn advance_terrain_water_region_fades(
    time: Res<Time>,
    phase: Res<State<GamePhase>>,
    mut regions: Query<&mut TerrainWaterRegionFade>,
    mut view: ResMut<openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterViewState>,
    training: Query<(), (
        With<openzt2_interface_state::immersive_modes::immersive_mode_state_types::FirstPersonTraining>,
        With<openzt2_interface_state::immersive_modes::immersive_mode_state_types::ActiveImmersiveMode>,
    )>,
) {
    if *phase.get() != GamePhase::InGame {
        return;
    }
    for mut fade in &mut regions {
        fade.advance(time.delta_secs());
    }
    view.advance_training_opacity(!training.is_empty(), time.delta_secs());
}
