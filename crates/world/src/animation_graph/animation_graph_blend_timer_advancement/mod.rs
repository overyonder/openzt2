use bevy::prelude::*;

use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;

#[derive(Component)]
pub(super) struct AdvancingAnimationGraphBlend;

pub(super) fn select_advancing_animation_graph_blends(
    mut commands: Commands,
    changed: Query<
        (
            Entity,
            &AnimationGraphPlaybackState,
            Has<AdvancingAnimationGraphBlend>,
        ),
        Changed<AnimationGraphPlaybackState>,
    >,
) {
    for (entity, state, advancing) in &changed {
        match (state.active_animation_graph_blend.is_some(), advancing) {
            (true, false) => {
                commands.entity(entity).insert(AdvancingAnimationGraphBlend);
            }
            (false, true) => {
                commands
                    .entity(entity)
                    .remove::<AdvancingAnimationGraphBlend>();
            }
            _ => {}
        }
    }
}

pub(crate) fn advance_animation_graph_blend_timers(
    mut commands: Commands,
    fixed_time: Res<Time<Fixed>>,
    mut animation_graph_playback_states: Query<
        (Entity, &mut AnimationGraphPlaybackState),
        With<AdvancingAnimationGraphBlend>,
    >,
) {
    for (entity, mut animation_graph_playback_state) in &mut animation_graph_playback_states {
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
            commands
                .entity(entity)
                .remove::<AdvancingAnimationGraphBlend>();
        }
    }
}
