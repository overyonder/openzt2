use bevy::prelude::*;

use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationCompleted;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use openzt2_zoo_state::topology::gate_operation_types::Gate;

use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;
use openzt2_world_state::animation_graph::animation_graph_transition_operations::enter_animation_graph_node_playback;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;

pub(crate) fn complete_animation_graph_transitions_after_animation_clip_completion(
    animation_set_assets: Res<Assets<AnimationSetAsset>>,
    mut completed_animations: MessageReader<AnimationCompleted>,
    gates: Query<(), With<Gate>>,
    mut animation_playback_controllers: Query<(
        &mut AnimationPlaybackController,
        &mut AnimationGraphPlaybackState,
        Option<&AnimationPresentationOwner>,
    )>,
) {
    for completed_animation in completed_animations.read() {
        let Ok((mut animation_playback_controller, mut animation_graph_playback_state, owner)) =
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
        // Gate state owns the finished open/closed pose until its next request.
        // Other one-shot clips retain their return to the containing graph node.
        if animation_playback_controller
            .explicit_clip_request_id
            .is_some()
            && animation_playback_controller.playback_repetition_policy
                == AnimationPlaybackRepetitionPolicy::PlayOnce
            && owner.is_some_and(|owner| gates.get(owner.gameplay_entity).is_ok())
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
            let route_owner = animation_graph_playback_state.route_owner.filter(|owner| {
                owner.playback_generation == animation_playback_controller.playback_generation
            });
            let remaining =
                std::mem::take(&mut animation_graph_playback_state.remaining_transition_nodes);
            let _ = enter_animation_graph_node_playback(
                animation_set_asset,
                active_animation_graph_transition.target_animation_graph_node_index,
                &mut animation_playback_controller,
                &mut animation_graph_playback_state,
                0,
            );
            animation_graph_playback_state.remaining_transition_nodes = remaining;
            let _ = openzt2_world_state::animation_graph::animation_graph_transition_operations::continue_animation_graph_transition_route(
                animation_set_asset, &mut animation_playback_controller, &mut animation_graph_playback_state,
            );
            animation_graph_playback_state.route_owner = route_owner.map(|mut owner| {
                owner.playback_generation = animation_playback_controller.playback_generation;
                owner
            });
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
