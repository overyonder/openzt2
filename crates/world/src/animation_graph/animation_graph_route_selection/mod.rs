use bevy::platform::collections::HashMap;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_world_state::animation_graph::{
    animation_graph_playback_state_types::AnimationGraphRoute,
    animation_graph_transition_operations::resolve_animation_graph_route,
};

pub(super) fn resolve_authored_animation_route(
    asset: &AnimationSetAsset,
    source: u32,
    target: u32,
) -> Option<AnimationGraphRoute> {
    let nodes = asset
        .animation_graph_nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            (
                node.animation_graph_node_asset_key.as_str(),
                u32::try_from(index).expect("animation graph node index fits u32"),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut adjacency = vec![Vec::new(); asset.animation_graph_nodes.len()];
    for (index, edge) in asset.animation_graph_edges.iter().enumerate() {
        if let Some((&from, &to)) = nodes
            .get(edge.source_animation_graph_node_asset_key.as_str())
            .zip(nodes.get(edge.target_animation_graph_node_asset_key.as_str()))
        {
            adjacency[from as usize].push((to, index));
        }
    }
    resolve_animation_graph_route(
        adjacency.len(),
        |node| adjacency[node as usize].as_slice(),
        source,
        target,
    )
}
