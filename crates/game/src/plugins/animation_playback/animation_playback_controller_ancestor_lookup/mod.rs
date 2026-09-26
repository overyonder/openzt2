use bevy::prelude::*;

use crate::plugins::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;

use super::animation_playback_controller_types::AnimationPlaybackController;

pub(super) fn find_animation_playback_controller_ancestor<'a>(
    mut descendant_entity: Entity,
    entity_parents: &Query<&ChildOf>,
    animation_playback_controllers: &'a Query<(
        &AnimationPlaybackController,
        Option<&AnimationGraphPlaybackState>,
    )>,
) -> Option<(
    Entity,
    &'a AnimationPlaybackController,
    Option<&'a AnimationGraphPlaybackState>,
)> {
    loop {
        if let Ok((animation_playback_controller, animation_graph_playback_state)) =
            animation_playback_controllers.get(descendant_entity)
        {
            return Some((
                descendant_entity,
                animation_playback_controller,
                animation_graph_playback_state,
            ));
        }
        descendant_entity = entity_parents.get(descendant_entity).ok()?.parent();
    }
}
