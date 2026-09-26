use bevy::prelude::*;
use openzt2_game_data::scene_prefab::PrefabRotationCycle;

#[derive(Component, Debug, Clone, Copy)]
pub(super) struct PrefabAuthoredRotationCycle(Vec3);

impl PrefabAuthoredRotationCycle {
    pub(super) fn from_authored_rotation_cycle(cycle: &PrefabRotationCycle) -> Self {
        Self(Vec3::from_array(cycle.radians_per_second.map(f32::from)))
    }
}

pub(super) fn advance_authored_prefab_rotation_cycles_from_elapsed_real_time(
    time: Res<Time<Real>>,
    mut cycles: Query<(&PrefabAuthoredRotationCycle, &mut Transform)>,
) {
    let delta = time.delta_secs();
    for (cycle, mut transform) in &mut cycles {
        transform.rotate(Quat::from_scaled_axis(cycle.0 * delta));
    }
}
