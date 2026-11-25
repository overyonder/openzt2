use bevy::prelude::*;

use super::terrain_water_geometric_wave_shader_state::{
    NativeTerrainWaterGeometricWaveShaderState, TerrainWaterWaveRegisters,
};

pub fn publish_terrain_water_wave_registers(
    mut commands: Commands,
    mut regions: Query<
        (
            &NativeTerrainWaterGeometricWaveShaderState,
            &mut TerrainWaterWaveRegisters,
        ),
        Changed<NativeTerrainWaterGeometricWaveShaderState>,
    >,
    mut retired: RemovedComponents<NativeTerrainWaterGeometricWaveShaderState>,
    remaining: Query<
        (),
        (
            With<TerrainWaterWaveRegisters>,
            Without<NativeTerrainWaterGeometricWaveShaderState>,
        ),
    >,
) {
    for (channels, mut registers) in &mut regions {
        registers.set_if_neq(channels.wave_registers());
    }
    for region in retired.read().filter(|&region| remaining.contains(region)) {
        commands
            .entity(region)
            .try_remove::<TerrainWaterWaveRegisters>();
    }
}
