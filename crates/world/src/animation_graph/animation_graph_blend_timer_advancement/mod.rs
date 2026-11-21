use bevy::prelude::*;

use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;

pub(crate) fn advance_animation_graph_blend_timers(
    fixed_time: Res<Time<Fixed>>,
    mut animation_graph_playback_states: Query<&mut AnimationGraphPlaybackState>,
) {
    for mut animation_graph_playback_state in &mut animation_graph_playback_states {
        let Some(active_animation_graph_blend) = animation_graph_playback_state
            .active_animation_graph_blend
            .as_mut()
        else {
            continue;
        };
        if active_animation_graph_blend
            .timer
            .tick(fixed_time.delta())
            .is_finished()
        {
            animation_graph_playback_state.active_animation_graph_blend = None;
        }
    }
}
