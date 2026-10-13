use bevy::prelude::*;
use openzt2_world_state::world_spawn::prefab_authored_rotation_cycle_advancement::PrefabAuthoredRotationCycle;

pub(super) fn advance_authored_prefab_rotation_cycles_from_elapsed_real_time(
    time: Res<Time<Real>>,
    mut cycles: Query<(&PrefabAuthoredRotationCycle, &mut Transform)>,
) {
    let delta = time.delta_secs();
    for (cycle, mut transform) in &mut cycles {
        cycle.advance_rotation(&mut transform, delta);
    }
}
