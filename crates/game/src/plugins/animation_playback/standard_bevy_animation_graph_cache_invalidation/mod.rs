use bevy::prelude::*;

use crate::assets::animation::animation_set_asset_types::AnimationSetAsset;

use super::standard_bevy_animation_player_projection_types::{
    StandardBevyAnimationGraphsByAnimationSetAsset, StandardBevyAnimationPlayerBinding,
};

pub(super) fn invalidate_standard_bevy_animation_graphs_and_player_bindings_after_asset_changes(
    mut commands: Commands,
    mut animation_set_asset_events: MessageReader<AssetEvent<AnimationSetAsset>>,
    mut standard_bevy_animation_graphs_by_animation_set_asset: ResMut<
        StandardBevyAnimationGraphsByAnimationSetAsset,
    >,
    animation_player_bindings: Query<(Entity, &StandardBevyAnimationPlayerBinding)>,
) {
    for animation_set_asset_event in animation_set_asset_events.read() {
        let changed_animation_set_asset_id = match animation_set_asset_event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::Removed { id }
            | AssetEvent::LoadedWithDependencies { id }
            | AssetEvent::Unused { id } => *id,
        };
        standard_bevy_animation_graphs_by_animation_set_asset
            .animation_graphs_by_animation_set_asset
            .remove(&changed_animation_set_asset_id);

        for (animation_player_entity, animation_player_binding) in &animation_player_bindings {
            if animation_player_binding.animation_set_asset_id == changed_animation_set_asset_id {
                commands
                    .entity(animation_player_entity)
                    .remove::<StandardBevyAnimationPlayerBinding>();
            }
        }
    }
}
