use bevy::prelude::*;

/// Authored presentation-graph gate. TEXTKEY `dis` and `en` commands change
/// this Bevy-owned state instead of leaving a source command interpreter alive.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimationGraphEnabled(pub bool);

impl Default for AnimationGraphEnabled {
    fn default() -> Self {
        Self(true)
    }
}

/// Per-entity cursor over one immutable authored animation graph.
///
/// This contains only playback state. Nodes, edges, clips, and keyframes remain
/// borrowed from the entity's shared animation asset.
#[derive(Component, Clone, Debug, Default)]
pub struct AnimationGraphPlaybackState {
    pub current_animation_graph_node_index: u32,
    pub active_animation_graph_transition: Option<AnimationGraphTransition>,
    pub active_animation_graph_blend: Option<AnimationGraphBlend>,
    pub remaining_transition_nodes: std::collections::VecDeque<u32>,
    pub route_owner: Option<AnimationGraphRouteOwner>,
}

#[derive(Clone, Copy, Debug)]
pub struct AnimationGraphRouteOwner {
    pub request_id: usize,
    pub target_node: u32,
    pub playback_generation: u64,
}

impl AnimationGraphPlaybackState {
    pub fn active_blend_duration_milliseconds(&self) -> Option<u32> {
        self.active_animation_graph_blend
            .as_ref()
            .map(|animation_graph_blend| animation_graph_blend.duration_milliseconds)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AnimationGraphTransition {
    pub target_animation_graph_node_index: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct AnimationGraphBlend {
    pub elapsed_milliseconds: u32,
    pub duration_milliseconds: u32,
}
