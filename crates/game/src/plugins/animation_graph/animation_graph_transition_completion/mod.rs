use bevy::prelude::*;

use crate::{
    assets::animation::animation_set_asset_types::AnimationSetAsset,
    plugins::animation_playback::{
        animation_event_message_types::AnimationCompleted,
        animation_playback_controller_types::AnimationPlaybackController,
    },
};

use super::{
    animation_graph_playback_state_types::AnimationGraphPlaybackState,
    animation_graph_transition_operations::enter_animation_graph_node_playback,
};

pub(crate) fn complete_animation_graph_transitions_after_animation_clip_completion(
    animation_set_assets: Res<Assets<AnimationSetAsset>>,
    mut completed_animations: MessageReader<AnimationCompleted>,
    mut animation_playback_controllers: Query<(
        &mut AnimationPlaybackController,
        &mut AnimationGraphPlaybackState,
    )>,
) {
    for completed_animation in completed_animations.read() {
        let Ok((mut animation_playback_controller, mut animation_graph_playback_state)) =
            animation_playback_controllers
                .get_mut(completed_animation.animation_playback_controller_entity)
        else {
            continue;
        };
        if completed_animation.playback_generation
            != animation_playback_controller.playback_generation
        {
            continue;
        }
        let Some(animation_set_asset) =
            animation_set_assets.get(&animation_playback_controller.animation_set_asset)
        else {
            continue;
        };
        if let Some(active_animation_graph_transition) = animation_graph_playback_state
            .active_animation_graph_transition
            .take()
        {
            let _ = enter_animation_graph_node_playback(
                animation_set_asset,
                active_animation_graph_transition.target_animation_graph_node_index,
                &mut animation_playback_controller,
                &mut animation_graph_playback_state,
                0,
            );
        } else {
            let current_animation_graph_node_index =
                animation_graph_playback_state.current_animation_graph_node_index;
            let _ = enter_animation_graph_node_playback(
                animation_set_asset,
                current_animation_graph_node_index,
                &mut animation_playback_controller,
                &mut animation_graph_playback_state,
                0,
            );
        }
    }
}
