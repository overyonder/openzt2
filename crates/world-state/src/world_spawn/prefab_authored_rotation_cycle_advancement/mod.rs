use bevy::prelude::*;
use openzt2_game_data::scene_prefab::PrefabRotationCycle;

#[derive(Component, Debug, Clone, Copy)]
pub struct PrefabAuthoredRotationCycle(Vec3);

impl PrefabAuthoredRotationCycle {
    pub fn from_authored_rotation_cycle(cycle: &PrefabRotationCycle) -> Self {
        Self(Vec3::from_array(cycle.radians_per_second.map(f32::from)))
    }
}

impl PrefabAuthoredRotationCycle {
    pub fn advance_rotation(&self, transform: &mut Transform, elapsed_seconds: f32) {
        transform.rotate(Quat::from_scaled_axis(self.0 * elapsed_seconds));
    }
}
