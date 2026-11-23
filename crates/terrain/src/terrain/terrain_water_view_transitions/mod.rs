use bevy::prelude::*;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_zoo_state::terrain::terrain_water_view_state::{
    TerrainWaterCameraPose, TerrainWaterRegionFade, TerrainWaterTrainingFade,
};

pub(super) fn advance_terrain_water_region_fades(
    time: Res<Time>,
    phase: Res<State<GamePhase>>,
    mut regions: Query<&mut TerrainWaterRegionFade>,
) {
    if *phase.get() != GamePhase::InGame {
        return;
    }
    for mut fade in &mut regions {
        let mut next = *fade;
        next.advance(time.delta_secs());
        fade.set_if_neq(next);
    }
}

pub(super) fn advance_terrain_water_training_fade(
    time: Res<Time>,
    mut cameras: Query<(&TerrainWaterCameraPose, &mut TerrainWaterTrainingFade), With<ZooCamera>>,
    training: Query<(), (
        With<openzt2_interface_state::immersive_modes::immersive_mode_state_types::FirstPersonTraining>,
        With<openzt2_interface_state::immersive_modes::immersive_mode_state_types::ActiveImmersiveMode>,
    )>,
) {
    let Ok((pose, mut fade)) = cameras.single_mut() else {
        return;
    };
    let next = if pose.0.is_none() {
        0.0
    } else {
        let rate = if training.is_empty() { -1.0 } else { 1.0 };
        (fade.0 + rate * time.delta_secs()).clamp(0.0, 1.0)
    };
    fade.set_if_neq(TerrainWaterTrainingFade(next));
}
