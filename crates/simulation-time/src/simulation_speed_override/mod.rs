use bevy::prelude::*;

/// Optional wall-clock acceleration for local gameplay verification.
#[derive(Resource)]
pub(crate) struct SimulationSpeedOverride(pub(crate) f32);

impl Default for SimulationSpeedOverride {
    fn default() -> Self {
        let Ok(value) = std::env::var("OPENZT2_SIMULATION_SPEED_MULTIPLIER") else {
            return Self(1.0);
        };
        match value.parse::<f32>() {
            Ok(multiplier) if multiplier.is_finite() && multiplier > 0.0 => {
                info!(multiplier, "applying simulation speed multiplier");
                Self(multiplier)
            }
            _ => {
                warn!(%value, "invalid simulation speed multiplier; using normal speed");
                Self(1.0)
            }
        }
    }
}
