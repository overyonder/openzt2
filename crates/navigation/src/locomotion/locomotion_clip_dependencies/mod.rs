use bevy::prelude::*;
use openzt2_game_assets::animation::{
    animation_clip_source_queries::AnimationClipSources,
    animation_set_asset_types::{AnimationClipSourceAsset, AnimationSetAsset},
};

/// Retains the authored clip sources needed to measure one controller's gait.
#[derive(Component)]
pub(super) struct LocomotionClipDependencies {
    animation_set: AssetId<AnimationSetAsset>,
    gait: String,
    sources: Vec<Handle<AnimationClipSourceAsset>>,
}

impl LocomotionClipDependencies {
    pub(super) fn retain(
        commands: &mut Commands,
        controller: Entity,
        current: Option<Mut<Self>>,
        animation_set: AssetId<AnimationSetAsset>,
        gait: &str,
        set: &AnimationSetAsset,
        sources: &AnimationClipSources,
    ) {
        if current
            .as_ref()
            .is_some_and(|current| current.animation_set == animation_set && current.gait == gait)
        {
            return;
        }
        let node = set
            .animation_graph_nodes
            .iter()
            .find(|node| node.animation_graph_node_asset_key == gait);
        let retained: Vec<_> = if let Some(node) = node {
            node.animation_clip_asset_keys
                .iter()
                .filter_map(|key| sources.request(set, key))
                .collect()
        } else {
            sources.request(set, gait).into_iter().collect()
        };
        if let Some(mut current) = current {
            current.animation_set = animation_set;
            current.gait.clear();
            current.gait.push_str(gait);
            current.sources = retained;
        } else {
            commands.entity(controller).insert(Self {
                animation_set,
                gait: gait.to_owned(),
                sources: retained,
            });
        }
    }
}
