use bevy::prelude::*;

use super::animation_graph_playback_state_types::AnimationGraphPlaybackState;

pub(crate) fn advance_animation_graph_blend_timers(
    fixed_time: Res<Time<Fixed>>,
    mut animation_graph_playback_states: Query<&mut AnimationGraphPlaybackState>,
) {
    let elapsed_milliseconds = fixed_time.delta().as_millis().min(u128::from(u32::MAX)) as u32;
    for mut animation_graph_playback_state in &mut animation_graph_playback_states {
        let Some(active_animation_graph_blend) = animation_graph_playback_state
            .active_animation_graph_blend
            .as_mut()
        else {
            continue;
        };
        active_animation_graph_blend.elapsed_milliseconds = active_animation_graph_blend
            .elapsed_milliseconds
            .saturating_add(elapsed_milliseconds);
        if active_animation_graph_blend.elapsed_milliseconds
            >= active_animation_graph_blend.duration_milliseconds
        {
            animation_graph_playback_state.active_animation_graph_blend = None;
        }
    }
}
