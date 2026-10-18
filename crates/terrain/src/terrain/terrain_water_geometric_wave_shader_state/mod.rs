use bevy::prelude::*;
use openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::{
    ActivateTerrainWaterImpactWave, NativeTerrainWaterGeometricWaveShaderState,
};

pub(super) fn apply_authored_water_impact_events_to_geometric_wave_channels(
    mut impacts: MessageReader<ActivateTerrainWaterImpactWave>,
    mut states: Query<&mut NativeTerrainWaterGeometricWaveShaderState>,
) {
    for impact in impacts.read() {
        if let Ok(mut state) = states.get_mut(impact.surface) {
            state.queue_native_impact_wave(impact.source_position, impact.native_strength);
        }
    }
}
