use bevy::prelude::*;
use openzt2_world_state::world_spawn::prefab_authored_rotation_cycle_advancement::PrefabRealTimeRotationClock;

pub(super) fn advance_authored_prefab_rotation_cycles_from_elapsed_real_time(
    time: Res<Time<Real>>,
    mut players: Query<(&mut PrefabRealTimeRotationClock, &mut AnimationPlayer)>,
) {
    for (mut clock, mut player) in &mut players {
        let Some(animation) = player.animation_mut(clock.node) else {
            continue;
        };
        let origin = *clock
            .origin
            .get_or_insert_with(|| time.elapsed().saturating_sub(time.delta()));
        animation.set_seek_time(time.elapsed().saturating_sub(origin).as_secs_f32());
    }
}
