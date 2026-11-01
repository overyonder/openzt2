use bevy::prelude::*;

use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;

use crate::animation_graph::animation_graph_transition_operations::select_preferred_animation_clip_from_graph_node;

/// `key` when it names a clip of the set, otherwise the clip a model starts in
/// when it enters the graph node named `key`. Authored data names nodes where
/// it means any of their clips, such as the dodo's `<ground behSet="Stand"/>`.
pub fn resolve_animation_clip_or_graph_node_asset_key(
    animation_set_asset: &AnimationSetAsset,
    key: &str,
) -> Option<String> {
    if animation_set_asset
        .find_animation_clip_record_by_asset_key(key)
        .is_some()
        || animation_set_asset
            .find_animation_clip_source_reference_by_asset_key(key)
            .is_some()
    {
        return Some(key.to_owned());
    }
    animation_set_asset
        .animation_graph_nodes
        .iter()
        .find(|node| node.animation_graph_node_asset_key == key)
        .and_then(|node| {
            select_initial_animation_clip_asset_key_from_graph_node(animation_set_asset, node)
        })
}

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

#[cfg(test)]
mod tests {
    use super::resolve_animation_clip_or_graph_node_asset_key;
    use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
    use openzt2_game_data::animation::animation_set::{
        AuthoredAnimationGraphNode, AuthoredAnimationSetClipReference,
    };

    #[test]
    fn graph_node_names_resolve_to_one_of_their_clips() {
        let mut set = AnimationSetAsset::create_ready_animation_set_asset(
            String::new(),
            String::new(),
            Vec::new(),
            vec![AuthoredAnimationGraphNode {
                animation_graph_node_asset_key: "Stand".to_owned(),
                animation_clip_asset_keys: vec!["Stand_Growl".to_owned(), "Stand_Idle".to_owned()],
                authored_animation_attributes: Vec::new(),
            }],
            Vec::new(),
            Vec::new(),
            None,
        );
        set.animation_clip_source_references = ["Stand_Growl", "Stand_Idle"]
            .map(|key| AuthoredAnimationSetClipReference {
                animation_clip_asset_key: key.to_owned(),
                animation_clip_asset_path: String::new(),
                playback_policy: Default::default(),
            })
            .to_vec();
        let resolve = |key| resolve_animation_clip_or_graph_node_asset_key(&set, key);
        assert_eq!(resolve("Stand").as_deref(), Some("Stand_Idle"));
        assert_eq!(resolve("Stand_Growl").as_deref(), Some("Stand_Growl"));
        assert_eq!(resolve("Sit"), None);
    }
}
