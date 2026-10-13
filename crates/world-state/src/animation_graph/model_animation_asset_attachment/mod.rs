use bevy::prelude::*;

use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;

use crate::animation_graph::animation_graph_transition_operations::select_preferred_animation_clip_from_graph_node;

/// The clip a model starts in when it enters `graph_node`, loaded or not.
pub fn select_initial_animation_clip_asset_key_from_graph_node(
    animation_set_asset: &AnimationSetAsset,
    graph_node: &openzt2_game_data::animation::animation_set::AuthoredAnimationGraphNode,
) -> Option<String> {
    select_preferred_animation_clip_from_graph_node(animation_set_asset, graph_node).or_else(|| {
        select_preferred_unloaded_animation_clip_from_graph_node(animation_set_asset, graph_node)
    })
}

fn select_preferred_unloaded_animation_clip_from_graph_node(
    animation_set_asset: &AnimationSetAsset,
    animation_graph_node: &openzt2_game_data::animation::animation_set::AuthoredAnimationGraphNode,
) -> Option<String> {
    let animation_clip_source_references_in_graph_node = || {
        animation_graph_node
            .animation_clip_asset_keys
            .iter()
            .filter_map(|animation_clip_asset_key| {
                animation_set_asset
                    .find_animation_clip_source_reference_by_asset_key(animation_clip_asset_key)
            })
    };
    animation_clip_source_references_in_graph_node()
        .find(|animation_clip| animation_clip.animation_clip_asset_key.ends_with("_Idle"))
        .or_else(|| {
            animation_clip_source_references_in_graph_node()
                .find(|animation_clip| animation_clip.animation_clip_asset_key.ends_with("_Ahead"))
        })
        .or_else(|| {
            animation_clip_source_references_in_graph_node().find(|animation_clip| {
                animation_clip.playback_policy.explicit_use_only != Some(true)
            })
        })
        .or_else(|| animation_clip_source_references_in_graph_node().next())
        .map(|animation_clip| animation_clip.animation_clip_asset_key.clone())
}
